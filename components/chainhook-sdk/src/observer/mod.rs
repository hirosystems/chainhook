mod http;
#[cfg(feature = "zeromq")]
mod zmq;

use crate::chainhooks::bitcoin::{
    evaluate_bitcoin_chainhooks_on_chain_event, handle_bitcoin_hook_action,
    BitcoinChainhookOccurrence, BitcoinChainhookOccurrencePayload, BitcoinTriggerChainhook,
};
use crate::chainhooks::stacks::{
    evaluate_stacks_chainhooks_on_chain_event, handle_stacks_hook_action,
    StacksChainhookOccurrence, StacksChainhookOccurrencePayload,
};
use crate::chainhooks::types::{
    ChainhookConfig, ChainhookFullSpecification, ChainhookSpecification,
};

use crate::indexer::bitcoin::{
    build_http_client, download_and_parse_block_with_retry, standardize_bitcoin_block,
    BitcoinBlockFullBreakdown,
};
use crate::indexer::{Indexer, IndexerConfig};
use crate::utils::{send_request, Context};

use bitcoincore_rpc::bitcoin::{BlockHash, Txid};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use chainhook_types::{
    BitcoinBlockData, BitcoinBlockSignaling, BitcoinChainEvent, BitcoinChainUpdatedWithBlocksData,
    BitcoinChainUpdatedWithReorgData, BitcoinNetwork, BlockIdentifier, BlockchainEvent,
    StacksChainEvent, StacksNetwork, StacksNodeConfig, TransactionIdentifier,
};
use hiro_system_kit;
use hiro_system_kit::slog;
use rocket::config::{self, Config, LogLevel};
use rocket::data::{Limits, ToByteUnit};
use rocket::serde::Deserialize;
use rocket::Shutdown;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::str;
use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_INGESTION_PORT: u16 = 20445;

#[derive(Deserialize)]
pub struct NewTransaction {
    pub txid: String,
    pub status: String,
    pub raw_result: String,
    pub raw_tx: String,
}

#[derive(Clone, Debug)]
pub enum Event {
    BitcoinChainEvent(BitcoinChainEvent),
    StacksChainEvent(StacksChainEvent),
}

pub enum DataHandlerEvent {
    Process(BitcoinChainhookOccurrencePayload),
    Terminate,
}

#[derive(Debug, Clone)]
pub struct EventObserverConfig {
    pub chainhook_config: Option<ChainhookConfig>,
    pub bitcoin_rpc_proxy_enabled: bool,
    pub ingestion_port: u16,
    pub bitcoind_rpc_username: String,
    pub bitcoind_rpc_password: String,
    pub bitcoind_rpc_url: String,
    pub bitcoin_block_signaling: BitcoinBlockSignaling,
    pub display_logs: bool,
    pub cache_path: String,
    pub bitcoin_network: BitcoinNetwork,
    pub stacks_network: StacksNetwork,
    pub data_handler_tx: Option<crossbeam_channel::Sender<DataHandlerEvent>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EventObserverConfigOverrides {
    pub ingestion_port: Option<u16>,
    pub bitcoind_rpc_username: Option<String>,
    pub bitcoind_rpc_password: Option<String>,
    pub bitcoind_rpc_url: Option<String>,
    pub bitcoind_zmq_url: Option<String>,
    pub stacks_node_rpc_url: Option<String>,
    pub display_logs: Option<bool>,
    pub cache_path: Option<String>,
    pub bitcoin_network: Option<String>,
    pub stacks_network: Option<String>,
}

impl EventObserverConfig {
    pub fn get_cache_path_buf(&self) -> PathBuf {
        let mut path_buf = PathBuf::new();
        path_buf.push(&self.cache_path);
        path_buf
    }

    pub fn get_bitcoin_config(&self) -> BitcoinConfig {
        let bitcoin_config = BitcoinConfig {
            username: self.bitcoind_rpc_username.clone(),
            password: self.bitcoind_rpc_password.clone(),
            rpc_url: self.bitcoind_rpc_url.clone(),
            network: self.bitcoin_network.clone(),
            bitcoin_block_signaling: self.bitcoin_block_signaling.clone(),
        };
        bitcoin_config
    }

    pub fn get_chainhook_store(&self) -> ChainhookStore {
        let mut chainhook_store = ChainhookStore::new();
        // If authorization not required, we create a default ChainhookConfig
        if let Some(ref chainhook_config) = self.chainhook_config {
            let mut chainhook_config = chainhook_config.clone();
            chainhook_store
                .predicates
                .stacks_chainhooks
                .append(&mut chainhook_config.stacks_chainhooks);
            chainhook_store
                .predicates
                .bitcoin_chainhooks
                .append(&mut chainhook_config.bitcoin_chainhooks);
        }
        chainhook_store
    }

    pub fn get_stacks_node_config(&self) -> &StacksNodeConfig {
        match self.bitcoin_block_signaling {
            BitcoinBlockSignaling::Stacks(ref config) => config,
            _ => unreachable!(),
        }
    }

    pub fn new_using_overrides(
        overrides: Option<&EventObserverConfigOverrides>,
    ) -> Result<EventObserverConfig, String> {
        let bitcoin_network =
            if let Some(network) = overrides.and_then(|c| c.bitcoin_network.as_ref()) {
                BitcoinNetwork::from_str(network)?
            } else {
                BitcoinNetwork::Regtest
            };

        let stacks_network =
            if let Some(network) = overrides.and_then(|c| c.stacks_network.as_ref()) {
                StacksNetwork::from_str(network)?
            } else {
                StacksNetwork::Devnet
            };

        let config = EventObserverConfig {
            bitcoin_rpc_proxy_enabled: false,
            chainhook_config: None,
            ingestion_port: overrides
                .and_then(|c| c.ingestion_port)
                .unwrap_or(DEFAULT_INGESTION_PORT),
            bitcoind_rpc_username: overrides
                .and_then(|c| c.bitcoind_rpc_username.clone())
                .unwrap_or("devnet".to_string()),
            bitcoind_rpc_password: overrides
                .and_then(|c| c.bitcoind_rpc_password.clone())
                .unwrap_or("devnet".to_string()),
            bitcoind_rpc_url: overrides
                .and_then(|c| c.bitcoind_rpc_url.clone())
                .unwrap_or("http://localhost:18443".to_string()),
            bitcoin_block_signaling: overrides
                .and_then(|c| c.bitcoind_zmq_url.as_ref())
                .map(|url| BitcoinBlockSignaling::ZeroMQ(url.clone()))
                .unwrap_or(BitcoinBlockSignaling::Stacks(
                    StacksNodeConfig::default_localhost(
                        overrides
                            .and_then(|c| c.ingestion_port)
                            .unwrap_or(DEFAULT_INGESTION_PORT),
                    ),
                )),
            display_logs: overrides.and_then(|c| c.display_logs).unwrap_or(false),
            cache_path: overrides
                .and_then(|c| c.cache_path.clone())
                .unwrap_or("cache".to_string()),
            bitcoin_network,
            stacks_network,
            data_handler_tx: None,
        };
        Ok(config)
    }
}

#[derive(Deserialize, Debug)]
pub struct ContractReadonlyCall {
    pub okay: bool,
    pub result: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObserverCommand {
    ProcessBitcoinBlock(BitcoinBlockFullBreakdown),
    CacheBitcoinBlock(BitcoinBlockData),
    PropagateBitcoinChainEvent(BlockchainEvent),
    PropagateStacksChainEvent(StacksChainEvent),
    PropagateStacksMempoolEvent(StacksChainMempoolEvent),
    RegisterPredicate(ChainhookFullSpecification),
    EnablePredicate(ChainhookSpecification),
    DeregisterBitcoinPredicate(String),
    DeregisterStacksPredicate(String),
    ExpireBitcoinPredicate(HookExpirationData),
    ExpireStacksPredicate(HookExpirationData),
    NotifyBitcoinTransactionProxied,
    Terminate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HookExpirationData {
    pub hook_uuid: String,
    pub block_height: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StacksChainMempoolEvent {
    TransactionsAdmitted(Vec<MempoolAdmissionData>),
    TransactionDropped(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MempoolAdmissionData {
    pub tx_data: String,
    pub tx_description: String,
}

#[derive(Clone, Debug)]
pub struct PredicateEvaluationReport {
    pub predicates_evaluated: BTreeMap<String, BTreeSet<BlockIdentifier>>,
    pub predicates_triggered: BTreeMap<String, BTreeSet<BlockIdentifier>>,
    pub predicates_expired: BTreeMap<String, BTreeSet<BlockIdentifier>>,
}

impl PredicateEvaluationReport {
    pub fn new() -> PredicateEvaluationReport {
        PredicateEvaluationReport {
            predicates_evaluated: BTreeMap::new(),
            predicates_triggered: BTreeMap::new(),
            predicates_expired: BTreeMap::new(),
        }
    }

    pub fn track_evaluation(&mut self, uuid: &str, block_identifier: &BlockIdentifier) {
        self.predicates_evaluated
            .entry(uuid.to_string())
            .and_modify(|e| {
                e.insert(block_identifier.clone());
            })
            .or_insert_with(|| {
                let mut set = BTreeSet::new();
                set.insert(block_identifier.clone());
                set
            });
    }

    pub fn track_trigger(&mut self, uuid: &str, blocks: &Vec<&BlockIdentifier>) {
        for block_id in blocks.into_iter() {
            self.predicates_triggered
                .entry(uuid.to_string())
                .and_modify(|e| {
                    e.insert((*block_id).clone());
                })
                .or_insert_with(|| {
                    let mut set = BTreeSet::new();
                    set.insert((*block_id).clone());
                    set
                });
        }
    }

    pub fn track_expiration(&mut self, uuid: &str, block_identifier: &BlockIdentifier) {
        self.predicates_expired
            .entry(uuid.to_string())
            .and_modify(|e| {
                e.insert(block_identifier.clone());
            })
            .or_insert_with(|| {
                let mut set = BTreeSet::new();
                set.insert(block_identifier.clone());
                set
            });
    }
}

#[derive(Clone, Debug)]
pub enum ObserverEvent {
    Error(String),
    Fatal(String),
    Info(String),
    BitcoinChainEvent((BitcoinChainEvent, PredicateEvaluationReport)),
    StacksChainEvent((StacksChainEvent, PredicateEvaluationReport)),
    NotifyBitcoinTransactionProxied,
    PredicateRegistered(ChainhookSpecification),
    PredicateDeregistered(ChainhookSpecification),
    PredicateEnabled(ChainhookSpecification),
    BitcoinPredicateTriggered(BitcoinChainhookOccurrencePayload),
    StacksPredicateTriggered(StacksChainhookOccurrencePayload),
    PredicatesTriggered(usize),
    Terminate,
    StacksChainMempoolEvent(StacksChainMempoolEvent),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// JSONRPC Request
pub struct BitcoinRPCRequest {
    /// The name of the RPC call
    pub method: String,
    /// Parameters to the RPC call
    pub params: serde_json::Value,
    /// Identifier for this Request, which should appear in the response
    pub id: serde_json::Value,
    /// jsonrpc field, MUST be "2.0"
    pub jsonrpc: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BitcoinConfig {
    pub username: String,
    pub password: String,
    pub rpc_url: String,
    pub network: BitcoinNetwork,
    pub bitcoin_block_signaling: BitcoinBlockSignaling,
}

#[derive(Debug, Clone)]
pub struct ChainhookStore {
    pub predicates: ChainhookConfig,
}

impl ChainhookStore {
    pub fn new() -> Self {
        Self {
            predicates: ChainhookConfig {
                stacks_chainhooks: vec![],
                bitcoin_chainhooks: vec![],
            },
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ReorgMetrics {
    timestamp: i64,
    applied_blocks: usize,
    rolled_back_blocks: usize,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ChainMetrics {
    pub tip_height: u64,
    pub last_reorg: Option<ReorgMetrics>,
    pub last_block_ingestion_at: u128,
    pub registered_predicates: usize,
    pub deregistered_predicates: usize,
}

impl ChainMetrics {
    pub fn deregister_prediate(&mut self) {
        self.registered_predicates -= 1;
        self.deregistered_predicates += 1;
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ObserverMetrics {
    pub bitcoin: ChainMetrics,
    pub stacks: ChainMetrics,
}

#[derive(Debug, Clone)]
pub struct BitcoinBlockDataCached {
    pub block: BitcoinBlockData,
    pub processed_by_sidecar: bool,
}

pub struct ObserverSidecar {
    pub bitcoin_blocks_mutator: Option<(
        crossbeam_channel::Sender<(Vec<BitcoinBlockDataCached>, Vec<BlockIdentifier>)>,
        crossbeam_channel::Receiver<Vec<BitcoinBlockDataCached>>,
    )>,
    pub bitcoin_chain_event_notifier: Option<crossbeam_channel::Sender<HandleBlock>>,
}

impl ObserverSidecar {
    fn perform_bitcoin_sidecar_mutations(
        &self,
        blocks: Vec<BitcoinBlockDataCached>,
        blocks_ids_to_rollback: Vec<BlockIdentifier>,
        ctx: &Context,
    ) -> Vec<BitcoinBlockDataCached> {
        if let Some(ref block_mutator) = self.bitcoin_blocks_mutator {
            ctx.try_log(|logger| slog::info!(logger, "Sending blocks to pre-processor",));
            let _ = block_mutator
                .0
                .send((blocks.clone(), blocks_ids_to_rollback));
            ctx.try_log(|logger| slog::info!(logger, "Waiting for blocks from pre-processor",));
            match block_mutator.1.recv() {
                Ok(updated_blocks) => {
                    ctx.try_log(|logger| slog::info!(logger, "Block received from pre-processor",));
                    updated_blocks
                }
                Err(e) => {
                    ctx.try_log(|logger| {
                        slog::error!(
                            logger,
                            "Unable to receive block from pre-processor {}",
                            e.to_string()
                        )
                    });
                    blocks
                }
            }
        } else {
            blocks
        }
    }

    fn notify_chain_event(&self, chain_event: &BitcoinChainEvent, _ctx: &Context) {
        if let Some(ref notifier) = self.bitcoin_chain_event_notifier {
            match chain_event {
                BitcoinChainEvent::ChainUpdatedWithBlocks(data) => {
                    for block in data.new_blocks.iter() {
                        let _ = notifier.send(HandleBlock::ApplyBlock(block.clone()));
                    }
                }
                BitcoinChainEvent::ChainUpdatedWithReorg(data) => {
                    for block in data.blocks_to_rollback.iter() {
                        let _ = notifier.send(HandleBlock::UndoBlock(block.clone()));
                    }
                    for block in data.blocks_to_apply.iter() {
                        let _ = notifier.send(HandleBlock::ApplyBlock(block.clone()));
                    }
                }
            }
        }
    }
}

pub fn start_event_observer(
    config: EventObserverConfig,
    observer_commands_tx: Sender<ObserverCommand>,
    observer_commands_rx: Receiver<ObserverCommand>,
    observer_events_tx: Option<crossbeam_channel::Sender<ObserverEvent>>,
    observer_sidecar: Option<ObserverSidecar>,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    match config.bitcoin_block_signaling {
        BitcoinBlockSignaling::ZeroMQ(ref url) => {
            ctx.try_log(|logger| {
                slog::info!(logger, "Observing Bitcoin chain events via ZeroMQ: {}", url)
            });
            let context_cloned = ctx.clone();
            let event_observer_config_moved = config.clone();
            let observer_commands_tx_moved = observer_commands_tx.clone();
            let _ = hiro_system_kit::thread_named("Chainhook event observer").spawn(move || {
                let future = start_bitcoin_event_observer(
                    event_observer_config_moved,
                    observer_commands_tx_moved,
                    observer_commands_rx,
                    observer_events_tx,
                    observer_sidecar,
                    context_cloned,
                );
                let _ = hiro_system_kit::nestable_block_on(future);
            });
        }
        BitcoinBlockSignaling::Stacks(ref _url) => {
            // Start chainhook event observer
            let context_cloned = ctx.clone();
            let event_observer_config_moved = config.clone();
            let observer_commands_tx_moved = observer_commands_tx.clone();
            let _ = hiro_system_kit::thread_named("Chainhook event observer").spawn(move || {
                let future = start_stacks_event_observer(
                    event_observer_config_moved,
                    observer_commands_tx_moved,
                    observer_commands_rx,
                    observer_events_tx,
                    observer_sidecar,
                    context_cloned,
                );
                let _ = hiro_system_kit::nestable_block_on(future);
            });

            ctx.try_log(|logger| {
                slog::info!(
                    logger,
                    "Listening on port {} for Stacks chain events",
                    config.get_stacks_node_config().ingestion_port
                )
            });

            ctx.try_log(|logger| {
                slog::info!(logger, "Observing Bitcoin chain events via Stacks node")
            });
        }
    }
    Ok(())
}

pub async fn start_bitcoin_event_observer(
    config: EventObserverConfig,
    _observer_commands_tx: Sender<ObserverCommand>,
    observer_commands_rx: Receiver<ObserverCommand>,
    observer_events_tx: Option<crossbeam_channel::Sender<ObserverEvent>>,
    observer_sidecar: Option<ObserverSidecar>,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    let chainhook_store = config.get_chainhook_store();

    let observer_metrics = ObserverMetrics {
        bitcoin: ChainMetrics {
            registered_predicates: 0,
            ..Default::default()
        },
        stacks: ChainMetrics {
            registered_predicates: 0,
            ..Default::default()
        },
    };
    let observer_metrics_rw_lock = Arc::new(RwLock::new(observer_metrics));

    #[cfg(feature = "zeromq")]
    {
        let ctx_moved = ctx.clone();
        let config_moved = config.clone();
        let _ = hiro_system_kit::thread_named("ZMQ handler").spawn(move || {
            let future =
                zmq::start_zeromq_runloop(&config_moved, _observer_commands_tx, &ctx_moved);
            let _ = hiro_system_kit::nestable_block_on(future);
        });
    }

    // This loop is used for handling background jobs, emitted by HTTP calls.
    start_observer_commands_handler(
        config,
        chainhook_store,
        observer_commands_rx,
        observer_events_tx,
        None,
        observer_metrics_rw_lock.clone(),
        observer_sidecar,
        ctx,
    )
    .await
}

pub async fn start_stacks_event_observer(
    config: EventObserverConfig,
    observer_commands_tx: Sender<ObserverCommand>,
    observer_commands_rx: Receiver<ObserverCommand>,
    observer_events_tx: Option<crossbeam_channel::Sender<ObserverEvent>>,
    observer_sidecar: Option<ObserverSidecar>,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    let indexer_config = IndexerConfig {
        bitcoind_rpc_url: config.bitcoind_rpc_url.clone(),
        bitcoind_rpc_username: config.bitcoind_rpc_username.clone(),
        bitcoind_rpc_password: config.bitcoind_rpc_password.clone(),
        stacks_network: StacksNetwork::Devnet,
        bitcoin_network: BitcoinNetwork::Regtest,
        bitcoin_block_signaling: config.bitcoin_block_signaling.clone(),
    };

    let indexer = Indexer::new(indexer_config.clone());

    let log_level = if config.display_logs {
        if cfg!(feature = "cli") {
            LogLevel::Critical
        } else {
            LogLevel::Debug
        }
    } else {
        LogLevel::Off
    };

    let ingestion_port = config.get_stacks_node_config().ingestion_port;
    let bitcoin_rpc_proxy_enabled = config.bitcoin_rpc_proxy_enabled;
    let bitcoin_config = config.get_bitcoin_config();

    let chainhook_store = config.get_chainhook_store();

    let indexer_rw_lock = Arc::new(RwLock::new(indexer));

    let background_job_tx_mutex = Arc::new(Mutex::new(observer_commands_tx.clone()));

    let observer_metrics = ObserverMetrics {
        bitcoin: ChainMetrics {
            registered_predicates: chainhook_store.predicates.bitcoin_chainhooks.len(),
            ..Default::default()
        },
        stacks: ChainMetrics {
            registered_predicates: chainhook_store.predicates.stacks_chainhooks.len(),
            ..Default::default()
        },
    };
    let observer_metrics_rw_lock = Arc::new(RwLock::new(observer_metrics));

    let limits = Limits::default().limit("json", 20.megabytes());
    let mut shutdown_config = config::Shutdown::default();
    shutdown_config.ctrlc = false;
    shutdown_config.grace = 0;
    shutdown_config.mercy = 0;

    let ingestion_config = Config {
        port: ingestion_port,
        workers: 3,
        address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        keep_alive: 5,
        temp_dir: std::env::temp_dir().into(),
        log_level: log_level.clone(),
        cli_colors: false,
        limits,
        shutdown: shutdown_config,
        ..Config::default()
    };

    let mut routes = rocket::routes![
        http::handle_ping,
        http::handle_new_bitcoin_block,
        http::handle_new_stacks_block,
        http::handle_new_microblocks,
        http::handle_new_mempool_tx,
        http::handle_drop_mempool_tx,
        http::handle_new_attachement,
        http::handle_mined_block,
        http::handle_mined_microblock,
    ];

    if bitcoin_rpc_proxy_enabled {
        routes.append(&mut routes![http::handle_bitcoin_rpc_call]);
        routes.append(&mut routes![http::handle_bitcoin_wallet_rpc_call]);
    }

    let ctx_cloned = ctx.clone();
    let ignite = rocket::custom(ingestion_config)
        .manage(indexer_rw_lock)
        .manage(background_job_tx_mutex)
        .manage(bitcoin_config)
        .manage(ctx_cloned)
        .manage(observer_metrics_rw_lock.clone())
        .mount("/", routes)
        .ignite()
        .await?;
    let ingestion_shutdown = Some(ignite.shutdown());

    let _ = std::thread::spawn(move || {
        let _ = hiro_system_kit::nestable_block_on(ignite.launch());
    });

    // This loop is used for handling background jobs, emitted by HTTP calls.
    start_observer_commands_handler(
        config,
        chainhook_store,
        observer_commands_rx,
        observer_events_tx,
        ingestion_shutdown,
        observer_metrics_rw_lock.clone(),
        observer_sidecar,
        ctx,
    )
    .await
}

pub fn get_bitcoin_proof(
    bitcoin_client_rpc: &Client,
    transaction_identifier: &TransactionIdentifier,
    block_identifier: &BlockIdentifier,
) -> Result<String, String> {
    let txid =
        Txid::from_str(&transaction_identifier.get_hash_bytes_str()).expect("unable to build txid");
    let block_hash =
        BlockHash::from_str(&block_identifier.hash[2..]).expect("unable to build block_hash");

    let res = bitcoin_client_rpc.get_tx_out_proof(&vec![txid], Some(&block_hash));
    match res {
        Ok(proof) => Ok(format!("0x{}", hex::encode(&proof))),
        Err(e) => Err(format!(
            "failed collecting proof for transaction {}: {}",
            transaction_identifier.hash,
            e.to_string()
        )),
    }
}

pub fn gather_proofs<'a>(
    trigger: &BitcoinTriggerChainhook<'a>,
    proofs: &mut HashMap<&'a TransactionIdentifier, String>,
    config: &EventObserverConfig,
    ctx: &Context,
) {
    let bitcoin_client_rpc = Client::new(
        &config.bitcoind_rpc_url,
        Auth::UserPass(
            config.bitcoind_rpc_username.to_string(),
            config.bitcoind_rpc_password.to_string(),
        ),
    )
    .expect("unable to build http client");

    for (transactions, block) in trigger.apply.iter() {
        for transaction in transactions.iter() {
            if !proofs.contains_key(&transaction.transaction_identifier) {
                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "Collecting proof for transaction {}",
                        transaction.transaction_identifier.hash
                    )
                });
                match get_bitcoin_proof(
                    &bitcoin_client_rpc,
                    &transaction.transaction_identifier,
                    &block.block_identifier,
                ) {
                    Ok(proof) => {
                        proofs.insert(&transaction.transaction_identifier, proof);
                    }
                    Err(e) => {
                        ctx.try_log(|logger| slog::error!(logger, "{e}"));
                    }
                }
            }
        }
    }
}

pub enum HandleBlock {
    ApplyBlock(BitcoinBlockData),
    UndoBlock(BitcoinBlockData),
}

pub async fn start_observer_commands_handler(
    config: EventObserverConfig,
    mut chainhook_store: ChainhookStore,
    observer_commands_rx: Receiver<ObserverCommand>,
    observer_events_tx: Option<crossbeam_channel::Sender<ObserverEvent>>,
    ingestion_shutdown: Option<Shutdown>,
    observer_metrics: Arc<RwLock<ObserverMetrics>>,
    observer_sidecar: Option<ObserverSidecar>,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    let mut chainhooks_occurrences_tracker: HashMap<String, u64> = HashMap::new();
    let networks = (&config.bitcoin_network, &config.stacks_network);
    let mut bitcoin_block_store: HashMap<BlockIdentifier, BitcoinBlockDataCached> = HashMap::new();
    let http_client = build_http_client();
    let store_update_required = observer_sidecar
        .as_ref()
        .and_then(|s| s.bitcoin_blocks_mutator.as_ref())
        .is_some();

    loop {
        let command = match observer_commands_rx.recv() {
            Ok(cmd) => cmd,
            Err(e) => {
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::Error(format!("Channel error: {:?}", e)));
                }
                continue;
            }
        };
        match command {
            ObserverCommand::Terminate => {
                ctx.try_log(|logger| slog::info!(logger, "Handling Termination command"));
                if let Some(ingestion_shutdown) = ingestion_shutdown {
                    ingestion_shutdown.notify();
                }
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::Info("Terminating event observer".into()));
                    let _ = tx.send(ObserverEvent::Terminate);
                }
                break;
            }
            ObserverCommand::ProcessBitcoinBlock(mut block_data) => {
                let block_hash = block_data.hash.to_string();
                let block = loop {
                    match standardize_bitcoin_block(
                        block_data.clone(),
                        &config.bitcoin_network,
                        &ctx,
                    ) {
                        Ok(block) => break block,
                        Err((e, retry)) => {
                            ctx.try_log(|logger| {
                                slog::error!(logger, "Error standardizing block: {}", e)
                            });
                            if retry {
                                block_data = match download_and_parse_block_with_retry(
                                    &http_client,
                                    &block_hash,
                                    &config.get_bitcoin_config(),
                                    &ctx,
                                )
                                .await
                                {
                                    Ok(block) => block,
                                    Err(e) => {
                                        ctx.try_log(|logger| {
                                            slog::warn!(
                                                logger,
                                                "unable to download_and_parse_block: {}",
                                                e.to_string()
                                            )
                                        });
                                        continue;
                                    }
                                };
                            }
                        }
                    };
                };
                match observer_metrics.write() {
                    Ok(mut metrics) => {
                        if block.block_identifier.index > metrics.bitcoin.tip_height {
                            metrics.bitcoin.tip_height = block.block_identifier.index;
                        }
                        metrics.bitcoin.last_block_ingestion_at = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .expect("Could not get current time in ms")
                            .as_millis()
                            .into();
                    }
                    Err(e) => ctx.try_log(|logger| {
                        slog::warn!(logger, "unable to acquire observer_metrics_rw_lock:{}", e)
                    }),
                };
                bitcoin_block_store.insert(
                    block.block_identifier.clone(),
                    BitcoinBlockDataCached {
                        block,
                        processed_by_sidecar: false,
                    },
                );
            }
            ObserverCommand::CacheBitcoinBlock(block) => {
                bitcoin_block_store.insert(
                    block.block_identifier.clone(),
                    BitcoinBlockDataCached {
                        block,
                        processed_by_sidecar: false,
                    },
                );
            }
            ObserverCommand::PropagateBitcoinChainEvent(blockchain_event) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling PropagateBitcoinChainEvent command")
                });
                let mut confirmed_blocks = vec![];

                // Update Chain event before propagation
                let chain_event = match blockchain_event {
                    BlockchainEvent::BlockchainUpdatedWithHeaders(data) => {
                        let mut blocks_to_mutate = vec![];
                        let mut new_blocks = vec![];

                        for header in data.new_headers.iter() {
                            if store_update_required {
                                let Some(block) =
                                    bitcoin_block_store.remove(&header.block_identifier)
                                else {
                                    continue;
                                };
                                blocks_to_mutate.push(block);
                            } else {
                                let Some(cache) = bitcoin_block_store.get(&header.block_identifier)
                                else {
                                    continue;
                                };
                                new_blocks.push(cache.block.clone());
                            };
                        }

                        if let Some(ref sidecar) = observer_sidecar {
                            let updated_blocks = sidecar.perform_bitcoin_sidecar_mutations(
                                blocks_to_mutate,
                                vec![],
                                &ctx,
                            );
                            for cache in updated_blocks.into_iter() {
                                bitcoin_block_store
                                    .insert(cache.block.block_identifier.clone(), cache.clone());
                                new_blocks.push(cache.block);
                            }
                        }

                        for header in data.confirmed_headers.iter() {
                            match bitcoin_block_store.remove(&header.block_identifier) {
                                Some(res) => {
                                    confirmed_blocks.push(res.block);
                                }
                                None => {
                                    ctx.try_log(|logger| {
                                        slog::error!(
                                            logger,
                                            "Unable to retrieve confirmed bitcoin block {}",
                                            header.block_identifier
                                        )
                                    });
                                }
                            }
                        }

                        BitcoinChainEvent::ChainUpdatedWithBlocks(
                            BitcoinChainUpdatedWithBlocksData {
                                new_blocks,
                                confirmed_blocks: confirmed_blocks.clone(),
                            },
                        )
                    }
                    BlockchainEvent::BlockchainUpdatedWithReorg(data) => {
                        let mut blocks_to_rollback = vec![];

                        let mut blocks_to_mutate = vec![];
                        let mut blocks_to_apply = vec![];

                        for header in data.headers_to_apply.iter() {
                            if store_update_required {
                                let Some(block) =
                                    bitcoin_block_store.remove(&header.block_identifier)
                                else {
                                    continue;
                                };
                                blocks_to_mutate.push(block);
                            } else {
                                let Some(cache) = bitcoin_block_store.get(&header.block_identifier)
                                else {
                                    continue;
                                };
                                blocks_to_apply.push(cache.block.clone());
                            };
                        }

                        let mut blocks_ids_to_rollback: Vec<BlockIdentifier> = vec![];

                        for header in data.headers_to_rollback.iter() {
                            match bitcoin_block_store.get(&header.block_identifier) {
                                Some(cache) => {
                                    blocks_ids_to_rollback.push(header.block_identifier.clone());
                                    blocks_to_rollback.push(cache.block.clone());
                                }
                                None => {
                                    ctx.try_log(|logger| {
                                        slog::error!(
                                            logger,
                                            "Unable to retrieve bitcoin block {}",
                                            header.block_identifier
                                        )
                                    });
                                }
                            }
                        }

                        if let Some(ref sidecar) = observer_sidecar {
                            let updated_blocks = sidecar.perform_bitcoin_sidecar_mutations(
                                blocks_to_mutate,
                                blocks_ids_to_rollback,
                                &ctx,
                            );
                            for cache in updated_blocks.into_iter() {
                                bitcoin_block_store
                                    .insert(cache.block.block_identifier.clone(), cache.clone());
                                blocks_to_apply.push(cache.block);
                            }
                        }

                        for header in data.confirmed_headers.iter() {
                            match bitcoin_block_store.remove(&header.block_identifier) {
                                Some(res) => {
                                    confirmed_blocks.push(res.block);
                                }
                                None => {
                                    ctx.try_log(|logger| {
                                        slog::error!(
                                            logger,
                                            "Unable to retrieve confirmed bitcoin block {}",
                                            header.block_identifier
                                        )
                                    });
                                }
                            }
                        }

                        match blocks_to_apply
                            .iter()
                            .max_by_key(|b| b.block_identifier.index)
                        {
                            Some(highest_tip_block) => match observer_metrics.write() {
                                Ok(mut metrics) => {
                                    metrics.bitcoin.last_reorg = Some(ReorgMetrics {
                                        timestamp: highest_tip_block.timestamp.into(),
                                        applied_blocks: blocks_to_apply.len(),
                                        rolled_back_blocks: blocks_to_rollback.len(),
                                    });
                                }
                                Err(e) => ctx.try_log(|logger| {
                                    slog::warn!(
                                        logger,
                                        "unable to acquire observer_metrics_rw_lock:{}",
                                        e
                                    )
                                }),
                            },
                            None => {}
                        }

                        BitcoinChainEvent::ChainUpdatedWithReorg(BitcoinChainUpdatedWithReorgData {
                            blocks_to_apply,
                            blocks_to_rollback,
                            confirmed_blocks: confirmed_blocks.clone(),
                        })
                    }
                };

                if let Some(ref sidecar) = observer_sidecar {
                    sidecar.notify_chain_event(&chain_event, &ctx)
                }
                // process hooks
                let mut hooks_ids_to_deregister = vec![];
                let mut requests = vec![];
                let mut report = PredicateEvaluationReport::new();

                let bitcoin_chainhooks = chainhook_store
                    .predicates
                    .bitcoin_chainhooks
                    .iter()
                    .filter(|p| p.enabled)
                    .filter(|p| p.expired_at.is_none())
                    .collect::<Vec<_>>();
                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "Evaluating {} bitcoin chainhooks registered",
                        bitcoin_chainhooks.len()
                    )
                });

                let (predicates_triggered, predicates_evaluated, predicates_expired) =
                    evaluate_bitcoin_chainhooks_on_chain_event(
                        &chain_event,
                        &bitcoin_chainhooks,
                        &ctx,
                    );

                for (uuid, block_identifier) in predicates_evaluated.into_iter() {
                    report.track_evaluation(uuid, block_identifier);
                }
                for (uuid, block_identifier) in predicates_expired.into_iter() {
                    report.track_expiration(uuid, block_identifier);
                }
                for entry in predicates_triggered.iter() {
                    let blocks_ids = entry
                        .apply
                        .iter()
                        .map(|e| &e.1.block_identifier)
                        .collect::<Vec<&BlockIdentifier>>();
                    report.track_trigger(&entry.chainhook.uuid, &blocks_ids);
                }

                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "{} bitcoin chainhooks positive evaluations",
                        predicates_triggered.len()
                    )
                });

                let mut chainhooks_to_trigger = vec![];

                for trigger in predicates_triggered.into_iter() {
                    let mut total_occurrences: u64 = *chainhooks_occurrences_tracker
                        .get(&trigger.chainhook.uuid)
                        .unwrap_or(&0);
                    // todo: this currently is only additive, and an occurrence means we match a chain event,
                    // rather than the number of blocks. Should we instead add to the total occurrences for
                    // every apply block, and subtract for every rollback? If we did this, we could set the
                    // status to `Expired` when we go above `expire_after_occurrence` occurrences, rather than
                    // deregistering
                    total_occurrences += 1;

                    let limit = trigger.chainhook.expire_after_occurrence.unwrap_or(0);
                    if limit == 0 || total_occurrences <= limit {
                        chainhooks_occurrences_tracker
                            .insert(trigger.chainhook.uuid.clone(), total_occurrences);
                        chainhooks_to_trigger.push(trigger);
                    } else {
                        hooks_ids_to_deregister.push(trigger.chainhook.uuid.clone());
                    }
                }

                let mut proofs = HashMap::new();
                for trigger in chainhooks_to_trigger.iter() {
                    if trigger.chainhook.include_proof {
                        gather_proofs(&trigger, &mut proofs, &config, &ctx);
                    }
                }

                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "{} bitcoin chainhooks will be triggered",
                        chainhooks_to_trigger.len()
                    )
                });

                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicatesTriggered(
                        chainhooks_to_trigger.len(),
                    ));
                }
                for chainhook_to_trigger in chainhooks_to_trigger.into_iter() {
                    match handle_bitcoin_hook_action(chainhook_to_trigger, &proofs) {
                        Err(e) => {
                            ctx.try_log(|logger| {
                                slog::error!(logger, "unable to handle action {}", e)
                            });
                        }
                        Ok(BitcoinChainhookOccurrence::Http(request, data)) => {
                            requests.push((request, data));
                        }
                        Ok(BitcoinChainhookOccurrence::File(_path, _bytes)) => {
                            ctx.try_log(|logger| {
                                slog::info!(logger, "Writing to disk not supported in server mode")
                            })
                        }
                        Ok(BitcoinChainhookOccurrence::Data(payload)) => {
                            if let Some(ref tx) = observer_events_tx {
                                let _ = tx.send(ObserverEvent::BitcoinPredicateTriggered(payload));
                            }
                        }
                    }
                }
                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "{} bitcoin chainhooks to deregister",
                        hooks_ids_to_deregister.len()
                    )
                });

                for hook_uuid in hooks_ids_to_deregister.iter() {
                    if let Some(chainhook) = chainhook_store
                        .predicates
                        .deregister_bitcoin_hook(hook_uuid.clone())
                    {
                        match observer_metrics.write() {
                            Ok(mut metrics) => metrics.bitcoin.deregister_prediate(),
                            Err(e) => ctx.try_log(|logger| {
                                slog::warn!(
                                    logger,
                                    "unable to acquire observer_metrics_rw_lock:{}",
                                    e
                                )
                            }),
                        }

                        if let Some(ref tx) = observer_events_tx {
                            let _ = tx.send(ObserverEvent::PredicateDeregistered(
                                ChainhookSpecification::Bitcoin(chainhook),
                            ));
                        }
                    }
                }

                for (request, data) in requests.into_iter() {
                    if send_request(request, 3, 1, &ctx).await.is_ok() {
                        if let Some(ref tx) = observer_events_tx {
                            let _ = tx.send(ObserverEvent::BitcoinPredicateTriggered(data));
                        }
                    }
                }

                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::BitcoinChainEvent((chain_event, report)));
                }
            }
            ObserverCommand::PropagateStacksChainEvent(chain_event) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling PropagateStacksChainEvent command")
                });
                let mut hooks_ids_to_deregister = vec![];
                let mut requests = vec![];
                let mut report = PredicateEvaluationReport::new();

                let stacks_chainhooks = chainhook_store
                    .predicates
                    .stacks_chainhooks
                    .iter()
                    .filter(|p| p.enabled)
                    .filter(|p| p.expired_at.is_none())
                    .collect::<Vec<_>>();
                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "Evaluating {} stacks chainhooks registered",
                        stacks_chainhooks.len()
                    )
                });
                // track stacks chain metrics
                match &chain_event {
                    StacksChainEvent::ChainUpdatedWithBlocks(update) => {
                        match update
                            .new_blocks
                            .iter()
                            .max_by_key(|b| b.block.block_identifier.index)
                        {
                            Some(highest_tip_update) => match observer_metrics.write() {
                                Ok(mut metrics) => {
                                    if highest_tip_update.block.block_identifier.index
                                        > metrics.stacks.tip_height
                                    {
                                        metrics.stacks.tip_height =
                                            highest_tip_update.block.block_identifier.index;
                                    }
                                    metrics.stacks.last_block_ingestion_at = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .expect("Could not get current time in ms")
                                        .as_millis()
                                        .into();
                                }
                                Err(e) => ctx.try_log(|logger| {
                                    slog::warn!(
                                        logger,
                                        "unable to acquire observer_metrics_rw_lock:{}",
                                        e
                                    )
                                }),
                            },
                            None => {}
                        }
                    }
                    StacksChainEvent::ChainUpdatedWithReorg(update) => {
                        match update
                            .blocks_to_apply
                            .iter()
                            .max_by_key(|b| b.block.block_identifier.index)
                        {
                            Some(highest_tip_update) => match observer_metrics.write() {
                                Ok(mut metrics) => {
                                    metrics.stacks.last_reorg = Some(ReorgMetrics {
                                        timestamp: highest_tip_update.block.timestamp.into(),
                                        applied_blocks: update.blocks_to_apply.len(),
                                        rolled_back_blocks: update.blocks_to_rollback.len(),
                                    });
                                }
                                Err(e) => ctx.try_log(|logger| {
                                    slog::warn!(
                                        logger,
                                        "unable to acquire observer_metrics_rw_lock:{}",
                                        e
                                    )
                                }),
                            },
                            None => {}
                        }
                    }
                    _ => {}
                }

                // process hooks
                let (predicates_triggered, predicates_evaluated, predicates_expired) =
                    evaluate_stacks_chainhooks_on_chain_event(
                        &chain_event,
                        stacks_chainhooks,
                        &ctx,
                    );
                for (uuid, block_identifier) in predicates_evaluated.into_iter() {
                    report.track_evaluation(uuid, block_identifier);
                }
                for (uuid, block_identifier) in predicates_expired.into_iter() {
                    report.track_expiration(uuid, block_identifier);
                }
                for entry in predicates_triggered.iter() {
                    let blocks_ids = entry
                        .apply
                        .iter()
                        .map(|e| e.1.get_identifier())
                        .collect::<Vec<&BlockIdentifier>>();
                    report.track_trigger(&entry.chainhook.uuid, &blocks_ids);
                }
                ctx.try_log(|logger| {
                    slog::info!(
                        logger,
                        "{} stacks chainhooks positive evaluations",
                        predicates_triggered.len()
                    )
                });

                let mut chainhooks_to_trigger = vec![];

                for trigger in predicates_triggered.into_iter() {
                    let mut total_occurrences: u64 = *chainhooks_occurrences_tracker
                        .get(&trigger.chainhook.uuid)
                        .unwrap_or(&0);
                    total_occurrences += 1;

                    let limit = trigger.chainhook.expire_after_occurrence.unwrap_or(0);
                    if limit == 0 || total_occurrences <= limit {
                        chainhooks_occurrences_tracker
                            .insert(trigger.chainhook.uuid.clone(), total_occurrences);
                        chainhooks_to_trigger.push(trigger);
                    } else {
                        hooks_ids_to_deregister.push(trigger.chainhook.uuid.clone());
                    }
                }

                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicatesTriggered(
                        chainhooks_to_trigger.len(),
                    ));
                }
                let proofs = HashMap::new();
                for chainhook_to_trigger in chainhooks_to_trigger.into_iter() {
                    match handle_stacks_hook_action(chainhook_to_trigger, &proofs, &ctx) {
                        Err(e) => {
                            ctx.try_log(|logger| {
                                slog::error!(logger, "unable to handle action {}", e)
                            });
                        }
                        Ok(StacksChainhookOccurrence::Http(request)) => {
                            requests.push(request);
                        }
                        Ok(StacksChainhookOccurrence::File(_path, _bytes)) => {
                            ctx.try_log(|logger| {
                                slog::info!(logger, "Writing to disk not supported in server mode")
                            })
                        }
                        Ok(StacksChainhookOccurrence::Data(payload)) => {
                            if let Some(ref tx) = observer_events_tx {
                                let _ = tx.send(ObserverEvent::StacksPredicateTriggered(payload));
                            }
                        }
                    }
                }

                for hook_uuid in hooks_ids_to_deregister.iter() {
                    if let Some(chainhook) = chainhook_store
                        .predicates
                        .deregister_stacks_hook(hook_uuid.clone())
                    {
                        match observer_metrics.write() {
                            Ok(mut metrics) => metrics.stacks.deregister_prediate(),

                            Err(e) => ctx.try_log(|logger| {
                                slog::warn!(
                                    logger,
                                    "unable to acquire observer_metrics_rw_lock:{}",
                                    e
                                )
                            }),
                        }

                        if let Some(ref tx) = observer_events_tx {
                            let _ = tx.send(ObserverEvent::PredicateDeregistered(
                                ChainhookSpecification::Stacks(chainhook),
                            ));
                        }
                    }
                }

                for request in requests.into_iter() {
                    // todo(lgalabru): collect responses for reporting
                    ctx.try_log(|logger| {
                        slog::info!(
                            logger,
                            "Dispatching request from stacks chainhook {:?}",
                            request
                        )
                    });
                    let _ = send_request(request, 3, 1, &ctx).await;
                }

                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::StacksChainEvent((chain_event, report)));
                }
            }
            ObserverCommand::PropagateStacksMempoolEvent(mempool_event) => {
                ctx.try_log(|logger| {
                    slog::debug!(logger, "Handling PropagateStacksMempoolEvent command")
                });
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::StacksChainMempoolEvent(mempool_event));
                }
            }
            ObserverCommand::NotifyBitcoinTransactionProxied => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling NotifyBitcoinTransactionProxied command")
                });
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::NotifyBitcoinTransactionProxied);
                }
            }
            ObserverCommand::RegisterPredicate(spec) => {
                ctx.try_log(|logger| slog::info!(logger, "Handling RegisterPredicate command"));

                let mut spec = match chainhook_store
                    .predicates
                    .register_full_specification(networks, spec)
                {
                    Ok(spec) => spec,
                    Err(e) => {
                        ctx.try_log(|logger| {
                            slog::error!(
                                logger,
                                "Unable to register new chainhook spec: {}",
                                e.to_string()
                            )
                        });
                        panic!("Unable to register new chainhook spec: {}", e.to_string());
                        //continue;
                    }
                };

                match observer_metrics.write() {
                    Ok(mut metrics) => match spec {
                        ChainhookSpecification::Bitcoin(_) => {
                            metrics.bitcoin.registered_predicates += 1
                        }
                        ChainhookSpecification::Stacks(_) => {
                            metrics.stacks.registered_predicates += 1
                        }
                    },
                    Err(e) => ctx.try_log(|logger| {
                        slog::warn!(logger, "unable to acquire observer_metrics_rw_lock:{}", e)
                    }),
                };

                ctx.try_log(|logger| slog::info!(logger, "Registering chainhook {}", spec.uuid(),));
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicateRegistered(spec.clone()));
                } else {
                    ctx.try_log(|logger| slog::info!(logger, "Enabling Predicate {}", spec.uuid()));
                    chainhook_store.predicates.enable_specification(&mut spec);
                }
            }
            ObserverCommand::EnablePredicate(mut spec) => {
                ctx.try_log(|logger| slog::info!(logger, "Enabling Predicate {}", spec.uuid()));
                chainhook_store.predicates.enable_specification(&mut spec);
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicateEnabled(spec));
                }
            }
            ObserverCommand::DeregisterStacksPredicate(hook_uuid) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling DeregisterStacksPredicate command")
                });
                let hook = chainhook_store.predicates.deregister_stacks_hook(hook_uuid);

                match observer_metrics.write() {
                    Ok(mut metrics) => metrics.stacks.deregister_prediate(),
                    Err(e) => ctx.try_log(|logger| {
                        slog::warn!(logger, "unable to acquire observer_metrics_rw_lock:{}", e)
                    }),
                }

                if let (Some(tx), Some(hook)) = (&observer_events_tx, hook) {
                    let _ = tx.send(ObserverEvent::PredicateDeregistered(
                        ChainhookSpecification::Stacks(hook),
                    ));
                }
            }
            ObserverCommand::DeregisterBitcoinPredicate(hook_uuid) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling DeregisterBitcoinPredicate command")
                });
                let hook = chainhook_store
                    .predicates
                    .deregister_bitcoin_hook(hook_uuid);

                match observer_metrics.write() {
                    Ok(mut metrics) => metrics.bitcoin.deregister_prediate(),
                    Err(e) => ctx.try_log(|logger| {
                        slog::warn!(logger, "unable to acquire observer_metrics_rw_lock:{}", e)
                    }),
                }

                if let (Some(tx), Some(hook)) = (&observer_events_tx, hook) {
                    let _ = tx.send(ObserverEvent::PredicateDeregistered(
                        ChainhookSpecification::Bitcoin(hook),
                    ));
                }
            }
            ObserverCommand::ExpireStacksPredicate(HookExpirationData {
                hook_uuid,
                block_height,
            }) => {
                ctx.try_log(|logger| slog::info!(logger, "Handling ExpireStacksPredicate command"));
                chainhook_store
                    .predicates
                    .expire_stacks_hook(hook_uuid, block_height);
            }
            ObserverCommand::ExpireBitcoinPredicate(HookExpirationData {
                hook_uuid,
                block_height,
            }) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling ExpireBitcoinPredicate command")
                });
                chainhook_store
                    .predicates
                    .expire_bitcoin_hook(hook_uuid, block_height);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub mod tests;
