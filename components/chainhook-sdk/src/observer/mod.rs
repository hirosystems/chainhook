#[cfg(feature = "stacks")]
pub mod stacks;
#[cfg(feature = "stacks")]
use self::stacks::{
    start_stacks_event_observer, StacksChainMempoolEvent, StacksObserverStartupContext,
};
#[cfg(feature = "stacks")]
use crate::chainhooks::stacks::{
    evaluate_stacks_chainhooks_on_chain_event, handle_stacks_hook_action,
    StacksChainhookOccurrence, StacksChainhookOccurrencePayload,
};
#[cfg(feature = "stacks")]
use chainhook_types::{BitcoinBlockSignaling, StacksChainEvent};

#[cfg(feature = "zeromq")]
mod zmq;

pub mod config;

use crate::chainhooks::bitcoin::{
    evaluate_bitcoin_chainhooks_on_chain_event, handle_bitcoin_hook_action,
    BitcoinChainhookOccurrence, BitcoinChainhookOccurrencePayload, BitcoinTriggerChainhook,
};

use crate::chainhooks::types::{
    ChainhookConfig, ChainhookFullSpecification, ChainhookSpecification,
};

use crate::indexer::bitcoin::{
    build_http_client, download_and_parse_block_with_retry, standardize_bitcoin_block,
    BitcoinBlockFullBreakdown,
};
use crate::monitoring::{start_serving_prometheus_metrics, PrometheusMonitoring};
use crate::utils::{send_request, Context};

use bitcoincore_rpc::bitcoin::{BlockHash, Txid};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use chainhook_types::{
    BitcoinBlockData, BitcoinChainEvent, BitcoinChainUpdatedWithBlocksData,
    BitcoinChainUpdatedWithReorgData, BlockIdentifier, BlockchainEvent, TransactionIdentifier,
};
use hiro_system_kit;
use hiro_system_kit::slog;
use rocket::serde::Deserialize;
use rocket::Shutdown;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::str;
use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};

use self::config::EventObserverConfig;

#[derive(Clone, Debug, PartialEq)]
pub enum ObserverCommand {
    ProcessBitcoinBlock(BitcoinBlockFullBreakdown),
    CacheBitcoinBlock(BitcoinBlockData),
    PropagateBitcoinChainEvent(BlockchainEvent),
    RegisterPredicate(ChainhookFullSpecification),
    EnablePredicate(ChainhookSpecification),
    DeregisterBitcoinPredicate(String),
    ExpireBitcoinPredicate(HookExpirationData),
    NotifyBitcoinTransactionProxied,
    Terminate,
    #[cfg(feature = "stacks")]
    PropagateStacksChainEvent(StacksChainEvent),
    #[cfg(feature = "stacks")]
    PropagateStacksMempoolEvent(StacksChainMempoolEvent),
    #[cfg(feature = "stacks")]
    DeregisterStacksPredicate(String),
    #[cfg(feature = "stacks")]
    ExpireStacksPredicate(HookExpirationData),
}

#[derive(Clone, Debug, PartialEq)]
pub struct HookExpirationData {
    pub hook_uuid: String,
    pub block_height: u64,
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
    #[cfg(feature = "stacks")]
    StacksChainEvent((StacksChainEvent, PredicateEvaluationReport)),
    NotifyBitcoinTransactionProxied,
    PredicateRegistered(ChainhookSpecification),
    PredicateDeregistered(String),
    PredicateEnabled(ChainhookSpecification),
    BitcoinPredicateTriggered(BitcoinChainhookOccurrencePayload),
    #[cfg(feature = "stacks")]
    StacksPredicateTriggered(StacksChainhookOccurrencePayload),
    PredicatesTriggered(usize),
    Terminate,
    #[cfg(feature = "stacks")]
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
pub struct ChainhookStore {
    pub predicates: ChainhookConfig,
}

impl ChainhookStore {
    pub fn new() -> Self {
        Self {
            predicates: ChainhookConfig {
                #[cfg(feature = "stacks")]
                stacks_chainhooks: vec![],
                bitcoin_chainhooks: vec![],
            },
        }
    }
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
    #[cfg(feature = "stacks")] stacks_startup_context: Option<StacksObserverStartupContext>,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    if cfg!(feature = "stacks") {
        #[cfg(feature = "stacks")]
        match config.bitcoin_block_signaling {
            BitcoinBlockSignaling::ZeroMQ(ref url) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Observing Bitcoin chain events via ZeroMQ: {}", url)
                });
                let context_cloned = ctx.clone();
                let event_observer_config_moved = config.clone();
                let observer_commands_tx_moved = observer_commands_tx.clone();
                let _ = hiro_system_kit::thread_named("Chainhook event observer")
                        .spawn(move || {
                            let future = start_bitcoin_event_observer(
                                event_observer_config_moved,
                                observer_commands_tx_moved,
                                observer_commands_rx,
                                observer_events_tx.clone(),
                                observer_sidecar,
                                context_cloned.clone(),
                            );
                            match hiro_system_kit::nestable_block_on(future) {
                                Ok(_) => {}
                                Err(e) => {
                                    if let Some(tx) = observer_events_tx {
                                        context_cloned.try_log(|logger| {
                                            slog::crit!(
                                                logger,
                                                "Chainhook event observer thread failed with error: {e}",
                                            )
                                        });
                                        let _ = tx.send(ObserverEvent::Terminate);
                                    }
                                }
                            }
                        })
                        .expect("unable to spawn thread");
            }
            BitcoinBlockSignaling::Stacks(ref _url) => {
                // Start chainhook event observer
                let context_cloned = ctx.clone();
                let event_observer_config_moved = config.clone();
                let observer_commands_tx_moved = observer_commands_tx.clone();

                let _ = hiro_system_kit::thread_named("Chainhook event observer")
                        .spawn(move || {
                            let future = start_stacks_event_observer(
                                event_observer_config_moved,
                                observer_commands_tx_moved,
                                observer_commands_rx,
                                observer_events_tx.clone(),
                                observer_sidecar,
                                stacks_startup_context.unwrap_or_default(),
                                context_cloned.clone(),
                            );
                            match hiro_system_kit::nestable_block_on(future) {
                                Ok(_) => {}
                                Err(e) => {
                                    if let Some(tx) = observer_events_tx {
                                        context_cloned.try_log(|logger| {
                                            slog::crit!(
                                                logger,
                                                "Chainhook event observer thread failed with error: {e}",
                                            )
                                        });
                                        let _ = tx.send(ObserverEvent::Terminate);
                                    }
                                }
                            }
                        })
                        .expect("unable to spawn thread");

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
    } else {
        #[cfg(not(feature = "stacks"))]
        ctx.try_log(|logger| {
            slog::info!(
                logger,
                "Observing Bitcoin chain events via ZeroMQ: {}",
                config.zmq_url
            )
        });
        let context_cloned = ctx.clone();
        let event_observer_config_moved = config.clone();
        let observer_commands_tx_moved = observer_commands_tx.clone();
        let _ = hiro_system_kit::thread_named("Chainhook event observer")
            .spawn(move || {
                let future = start_bitcoin_event_observer(
                    event_observer_config_moved,
                    observer_commands_tx_moved,
                    observer_commands_rx,
                    observer_events_tx.clone(),
                    observer_sidecar,
                    context_cloned.clone(),
                );
                match hiro_system_kit::nestable_block_on(future) {
                    Ok(_) => {}
                    Err(e) => {
                        if let Some(tx) = observer_events_tx {
                            context_cloned.try_log(|logger| {
                                slog::crit!(
                                    logger,
                                    "Chainhook event observer thread failed with error: {e}",
                                )
                            });
                            let _ = tx.send(ObserverEvent::Terminate);
                        }
                    }
                }
            })
            .expect("unable to spawn thread");
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

    #[cfg(feature = "stacks")]
    let registered_stacks_chainhooks = chainhook_store.predicates.stacks_chainhooks.len() as u64;
    #[cfg(not(feature = "stacks"))]
    let registered_stacks_chainhooks = 0;

    let prometheus_monitoring = PrometheusMonitoring::new();
    prometheus_monitoring.initialize(
        registered_stacks_chainhooks,
        chainhook_store.predicates.bitcoin_chainhooks.len() as u64,
        None,
    );

    if let Some(port) = config.prometheus_monitoring_port {
        let registry_moved = prometheus_monitoring.registry.clone();
        let ctx_cloned = ctx.clone();
        let _ = std::thread::spawn(move || {
            let _ = hiro_system_kit::nestable_block_on(start_serving_prometheus_metrics(
                port,
                registry_moved,
                ctx_cloned,
            ));
        });
    }

    // This loop is used for handling background jobs, emitted by HTTP calls.
    start_observer_commands_handler(
        config,
        chainhook_store,
        observer_commands_rx,
        observer_events_tx,
        None,
        prometheus_monitoring,
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
                    slog::debug!(
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
                        ctx.try_log(|logger| slog::warn!(logger, "{e}"));
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
    prometheus_monitoring: PrometheusMonitoring,
    observer_sidecar: Option<ObserverSidecar>,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    let mut chainhooks_occurrences_tracker: HashMap<String, u64> = HashMap::new();
    let bitcoin_network = &config.bitcoin_network;
    #[cfg(feature = "stacks")]
    let stacks_network = &config.stacks_network;
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
                ctx.try_log(|logger| {
                    slog::crit!(logger, "Error: broken channel {}", e.to_string())
                });
                break;
            }
        };
        match command {
            ObserverCommand::Terminate => {
                break;
            }
            ObserverCommand::ProcessBitcoinBlock(mut block_data) => {
                let block_hash = block_data.hash.to_string();
                let mut attempts = 0;
                let max_attempts = 10;
                let block = loop {
                    match standardize_bitcoin_block(block_data.clone(), bitcoin_network, &ctx) {
                        Ok(block) => break Some(block),
                        Err((e, refetch_block)) => {
                            attempts += 1;
                            if attempts > max_attempts {
                                break None;
                            }
                            ctx.try_log(|logger| {
                                slog::warn!(logger, "Error standardizing block: {}", e)
                            });
                            if refetch_block {
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
                let Some(block) = block else {
                    ctx.try_log(|logger| {
                        slog::crit!(
                            logger,
                            "Could not process bitcoin block after {} attempts.",
                            attempts
                        )
                    });
                    break;
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
                let (chain_event, new_tip) = match blockchain_event {
                    BlockchainEvent::BlockchainUpdatedWithHeaders(data) => {
                        let mut blocks_to_mutate = vec![];
                        let mut new_blocks = vec![];
                        let mut new_tip = 0;

                        for header in data.new_headers.iter() {
                            if header.block_identifier.index > new_tip {
                                new_tip = header.block_identifier.index;
                            }

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

                        (
                            BitcoinChainEvent::ChainUpdatedWithBlocks(
                                BitcoinChainUpdatedWithBlocksData {
                                    new_blocks,
                                    confirmed_blocks: confirmed_blocks.clone(),
                                },
                            ),
                            new_tip,
                        )
                    }
                    BlockchainEvent::BlockchainUpdatedWithReorg(data) => {
                        let mut blocks_to_rollback = vec![];

                        let mut blocks_to_mutate = vec![];
                        let mut blocks_to_apply = vec![];
                        let mut new_tip = 0;

                        for header in data.headers_to_apply.iter() {
                            if header.block_identifier.index > new_tip {
                                new_tip = header.block_identifier.index;
                            }

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
                            Some(highest_tip_block) => {
                                prometheus_monitoring.btc_metrics_set_reorg(
                                    highest_tip_block.timestamp.into(),
                                    blocks_to_apply.len() as u64,
                                    blocks_to_rollback.len() as u64,
                                );
                            }
                            None => {}
                        }

                        (
                            BitcoinChainEvent::ChainUpdatedWithReorg(
                                BitcoinChainUpdatedWithReorgData {
                                    blocks_to_apply,
                                    blocks_to_rollback,
                                    confirmed_blocks: confirmed_blocks.clone(),
                                },
                            ),
                            new_tip,
                        )
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
                    let predicate_uuid = &chainhook_to_trigger.chainhook.uuid;
                    match handle_bitcoin_hook_action(chainhook_to_trigger, &proofs) {
                        Err(e) => {
                            ctx.try_log(|logger| {
                                slog::warn!(
                                    logger,
                                    "unable to handle action for predicate {}: {}",
                                    predicate_uuid,
                                    e
                                )
                            });
                        }
                        Ok(BitcoinChainhookOccurrence::Http(request, data)) => {
                            requests.push((request, data));
                        }
                        Ok(BitcoinChainhookOccurrence::File(_path, _bytes)) => {
                            ctx.try_log(|logger| {
                                slog::warn!(logger, "Writing to disk not supported in server mode")
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
                    if chainhook_store
                        .predicates
                        .deregister_bitcoin_hook(hook_uuid.clone())
                        .is_some()
                    {
                        prometheus_monitoring.btc_metrics_deregister_predicate();
                    }
                    if let Some(ref tx) = observer_events_tx {
                        let _ = tx.send(ObserverEvent::PredicateDeregistered(hook_uuid.clone()));
                    }
                }

                for (request, data) in requests.into_iter() {
                    // todo: need to handle failure case - we should be setting interrupted status: https://github.com/hirosystems/chainhook/issues/523
                    if send_request(request, 3, 1, &ctx).await.is_ok() {
                        if let Some(ref tx) = observer_events_tx {
                            let _ = tx.send(ObserverEvent::BitcoinPredicateTriggered(data));
                        }
                    }
                }

                prometheus_monitoring.btc_metrics_block_evaluated(new_tip);

                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::BitcoinChainEvent((chain_event, report)));
                }
            }
            #[cfg(feature = "stacks")]
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
                let new_tip = match &chain_event {
                    StacksChainEvent::ChainUpdatedWithBlocks(update) => {
                        match update
                            .new_blocks
                            .iter()
                            .max_by_key(|b| b.block.block_identifier.index)
                        {
                            Some(highest_tip_update) => {
                                highest_tip_update.block.block_identifier.index
                            }
                            None => 0,
                        }
                    }
                    StacksChainEvent::ChainUpdatedWithReorg(update) => {
                        match update
                            .blocks_to_apply
                            .iter()
                            .max_by_key(|b| b.block.block_identifier.index)
                        {
                            Some(highest_tip_update) => {
                                prometheus_monitoring.stx_metrics_set_reorg(
                                    highest_tip_update.block.timestamp,
                                    update.blocks_to_apply.len() as u64,
                                    update.blocks_to_rollback.len() as u64,
                                );
                                highest_tip_update.block.block_identifier.index
                            }
                            None => 0,
                        }
                    }
                    _ => 0,
                };

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
                    let predicate_uuid = &chainhook_to_trigger.chainhook.uuid;
                    match handle_stacks_hook_action(chainhook_to_trigger, &proofs, &ctx) {
                        Err(e) => {
                            ctx.try_log(|logger| {
                                slog::warn!(
                                    logger,
                                    "unable to handle action for predicate {}: {}",
                                    predicate_uuid,
                                    e
                                )
                            });
                        }
                        Ok(StacksChainhookOccurrence::Http(request)) => {
                            requests.push(request);
                        }
                        Ok(StacksChainhookOccurrence::File(_path, _bytes)) => {
                            ctx.try_log(|logger| {
                                slog::warn!(logger, "Writing to disk not supported in server mode")
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
                    if chainhook_store
                        .predicates
                        .deregister_stacks_hook(hook_uuid.clone())
                        .is_some()
                    {
                        prometheus_monitoring.stx_metrics_deregister_predicate();
                    }
                    if let Some(ref tx) = observer_events_tx {
                        let _ = tx.send(ObserverEvent::PredicateDeregistered(hook_uuid.clone()));
                    }
                }

                for request in requests.into_iter() {
                    // todo(lgalabru): collect responses for reporting
                    ctx.try_log(|logger| {
                        slog::debug!(
                            logger,
                            "Dispatching request from stacks chainhook {:?}",
                            request
                        )
                    });
                    let _ = send_request(request, 3, 1, &ctx).await;
                }

                prometheus_monitoring.stx_metrics_block_evaluated(new_tip);

                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::StacksChainEvent((chain_event, report)));
                }
            }
            #[cfg(feature = "stacks")]
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
                    slog::debug!(logger, "Handling NotifyBitcoinTransactionProxied command")
                });
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::NotifyBitcoinTransactionProxied);
                }
            }
            ObserverCommand::RegisterPredicate(spec) => {
                ctx.try_log(|logger| slog::info!(logger, "Handling RegisterPredicate command"));

                let mut spec = match chainhook_store.predicates.register_full_specification(
                    bitcoin_network,
                    #[cfg(feature = "stacks")]
                    stacks_network,
                    spec,
                ) {
                    Ok(spec) => spec,
                    Err(e) => {
                        ctx.try_log(|logger| {
                            slog::warn!(
                                logger,
                                "Unable to register new chainhook spec: {}",
                                e.to_string()
                            )
                        });
                        continue;
                    }
                };

                match spec {
                    ChainhookSpecification::Bitcoin(_) => {
                        prometheus_monitoring.btc_metrics_register_predicate()
                    }
                    #[cfg(feature = "stacks")]
                    ChainhookSpecification::Stacks(_) => {
                        prometheus_monitoring.stx_metrics_register_predicate()
                    }
                };

                ctx.try_log(
                    |logger| slog::debug!(logger, "Registering chainhook {}", spec.uuid(),),
                );
                if let Some(ref tx) = observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicateRegistered(spec.clone()));
                } else {
                    ctx.try_log(|logger| {
                        slog::debug!(logger, "Enabling Predicate {}", spec.uuid())
                    });
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
            #[cfg(feature = "stacks")]
            ObserverCommand::DeregisterStacksPredicate(hook_uuid) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling DeregisterStacksPredicate command")
                });
                let hook = chainhook_store
                    .predicates
                    .deregister_stacks_hook(hook_uuid.clone());

                if hook.is_some() {
                    // on startup, only the predicates in the `chainhook_store` are added to the monitoring count,
                    // so only those that we find in the store should be removed
                    prometheus_monitoring.stx_metrics_deregister_predicate();
                };
                // event if the predicate wasn't in the `chainhook_store`, propogate this event to delete from redis
                if let Some(tx) = &observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicateDeregistered(hook_uuid));
                };
            }
            ObserverCommand::DeregisterBitcoinPredicate(hook_uuid) => {
                ctx.try_log(|logger| {
                    slog::info!(logger, "Handling DeregisterBitcoinPredicate command")
                });
                let hook = chainhook_store
                    .predicates
                    .deregister_bitcoin_hook(hook_uuid.clone());

                if hook.is_some() {
                    // on startup, only the predicates in the `chainhook_store` are added to the monitoring count,
                    // so only those that we find in the store should be removed
                    prometheus_monitoring.btc_metrics_deregister_predicate();
                };
                // event if the predicate wasn't in the `chainhook_store`, propogate this event to delete from redis
                if let Some(tx) = &observer_events_tx {
                    let _ = tx.send(ObserverEvent::PredicateDeregistered(hook_uuid));
                };
            }
            #[cfg(feature = "stacks")]
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
    terminate(ingestion_shutdown, observer_events_tx, &ctx);
    Ok(())
}

fn terminate(
    ingestion_shutdown: Option<Shutdown>,
    observer_events_tx: Option<crossbeam_channel::Sender<ObserverEvent>>,
    ctx: &Context,
) {
    ctx.try_log(|logger| slog::info!(logger, "Handling Termination command"));
    if let Some(ingestion_shutdown) = ingestion_shutdown {
        ingestion_shutdown.notify();
    }
    if let Some(ref tx) = observer_events_tx {
        let _ = tx.send(ObserverEvent::Info("Terminating event observer".into()));
        let _ = tx.send(ObserverEvent::Terminate);
    }
}
#[cfg(test)]
pub mod tests;
