mod http_api;
mod runloops;

use crate::cli::fetch_and_standardize_block;
use crate::config::{Config, PredicatesApi, PredicatesApiConfig};
use crate::hord::should_sync_hord_db;
use crate::scan::bitcoin::process_block_with_predicates;
use crate::scan::stacks::consolidate_local_stacks_chainstate_using_csv;
use crate::service::http_api::{load_predicates_from_redis, start_predicate_api_server};
use crate::service::runloops::{start_bitcoin_scan_runloop, start_stacks_scan_runloop};
use crate::storage::{
    confirm_entries_in_stacks_blocks, draft_entries_in_stacks_blocks, open_readwrite_stacks_db_conn,
};

use chainhook_sdk::bitcoincore_rpc::{Auth, Client, RpcApi};
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::hashes::hex::FromHex;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::{Address, Network, Script};
use chainhook_sdk::chainhooks::types::{
    BitcoinChainhookSpecification, ChainhookConfig, ChainhookFullSpecification,
};

use chainhook_sdk::chainhooks::types::ChainhookSpecification;
use chainhook_sdk::hord::db::{
    find_all_inscriptions_in_block, find_latest_inscription_block_height, format_satpoint_to_watch,
    insert_entry_in_locations, open_readonly_hord_db_conn, open_readwrite_hord_db_conn,
    parse_satpoint_to_watch, remove_entries_from_locations_at_block_height,
};
use chainhook_sdk::hord::{
    update_storage_and_augment_bitcoin_block_with_inscription_transfer_data, Storage,
};
use chainhook_sdk::observer::{start_event_observer, BitcoinConfig, ObserverEvent};
use chainhook_sdk::utils::Context;
use chainhook_types::{
    BitcoinBlockData, BitcoinBlockSignaling, BitcoinNetwork, OrdinalInscriptionTransferData,
    OrdinalOperation, StacksChainEvent,
};
use redis::{Commands, Connection};

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Sender};

pub struct Service {
    config: Config,
    ctx: Context,
}

impl Service {
    pub fn new(config: Config, ctx: Context) -> Self {
        Self { config, ctx }
    }

    pub async fn run(
        &mut self,
        predicates: Vec<ChainhookFullSpecification>,
        hord_disabled: bool,
    ) -> Result<(), String> {
        let mut chainhook_config = ChainhookConfig::new();

        // If no predicates passed at launch, retrieve predicates from Redis
        if predicates.is_empty() && self.config.is_http_api_enabled() {
            let registered_predicates = match load_predicates_from_redis(&self.config, &self.ctx) {
                Ok(predicates) => predicates,
                Err(e) => {
                    error!(
                        self.ctx.expect_logger(),
                        "Failed loading predicate from storage: {}",
                        e.to_string()
                    );
                    vec![]
                }
            };
            for (predicate, _status) in registered_predicates.into_iter() {
                let predicate_uuid = predicate.uuid().to_string();
                match chainhook_config.register_specification(predicate) {
                    Ok(_) => {
                        info!(
                            self.ctx.expect_logger(),
                            "Predicate {} retrieved from storage and loaded", predicate_uuid,
                        );
                    }
                    Err(e) => {
                        error!(
                            self.ctx.expect_logger(),
                            "Failed loading predicate from storage: {}",
                            e.to_string()
                        );
                    }
                }
            }
        }

        // For each predicate found, register in memory.
        for predicate in predicates.into_iter() {
            match chainhook_config.register_full_specification(
                (
                    &self.config.network.bitcoin_network,
                    &self.config.network.stacks_network,
                ),
                predicate,
            ) {
                Ok(spec) => {
                    info!(
                        self.ctx.expect_logger(),
                        "Predicate {} retrieved from config and loaded",
                        spec.uuid(),
                    );
                }
                Err(e) => {
                    error!(
                        self.ctx.expect_logger(),
                        "Failed loading predicate from config: {}",
                        e.to_string()
                    );
                }
            }
        }

        let (observer_command_tx, observer_command_rx) = channel();
        let (observer_event_tx, observer_event_rx) = crossbeam_channel::unbounded();
        // let (ordinal_indexer_command_tx, ordinal_indexer_command_rx) = channel();

        let mut event_observer_config = self.config.get_event_observer_config();
        event_observer_config.chainhook_config = Some(chainhook_config);
        event_observer_config.hord_config = match hord_disabled {
            true => None,
            false => Some(self.config.get_hord_config()),
        };

        // Download and ingest a Stacks dump
        if self.config.rely_on_remote_stacks_tsv() {
            let _ =
                consolidate_local_stacks_chainstate_using_csv(&mut self.config, &self.ctx).await;
        }

        // Download and ingest a Ordinal dump, if hord is enabled
        if !hord_disabled {
            // TODO: add flag
            // let _ = download_ordinals_dataset_if_required(&mut self.config, &self.ctx).await;
            info!(
                self.ctx.expect_logger(),
                "Ordinal indexing is enabled by default, checking index... (use --no-hord to disable ordinals)"
            );

            let (tx, rx) = channel();

            let mut moved_event_observer_config = event_observer_config.clone();
            let moved_ctx = self.ctx.clone();

            let _ = hiro_system_kit::thread_named("Initial predicate processing")
                .spawn(move || {
                    if let Some(mut chainhook_config) =
                        moved_event_observer_config.chainhook_config.take()
                    {
                        let mut bitcoin_predicates_ref: Vec<&BitcoinChainhookSpecification> =
                            vec![];
                        for bitcoin_predicate in chainhook_config.bitcoin_chainhooks.iter_mut() {
                            bitcoin_predicate.enabled = false;
                            bitcoin_predicates_ref.push(bitcoin_predicate);
                        }
                        while let Ok(block) = rx.recv() {
                            let future = process_block_with_predicates(
                                block,
                                &bitcoin_predicates_ref,
                                &moved_event_observer_config,
                                &moved_ctx,
                            );
                            let res = hiro_system_kit::nestable_block_on(future);
                            if let Err(_) = res {
                                error!(moved_ctx.expect_logger(), "Initial ingestion failing");
                            }
                        }
                    }
                })
                .expect("unable to spawn thread");

            let inscriptions_db_conn =
                open_readonly_hord_db_conn(&self.config.expected_cache_path(), &self.ctx)?;

            let end_block =
                match find_latest_inscription_block_height(&inscriptions_db_conn, &self.ctx)? {
                    Some(height) => height,
                    None => panic!(),
                };

            self.replay_transfers(784628, end_block, Some(tx.clone()))?;

            while let Some((start_block, end_block)) = should_sync_hord_db(&self.config, &self.ctx)?
            {
                if start_block == 0 {
                    info!(
                        self.ctx.expect_logger(),
                        "Initializing hord indexing from block #{}", start_block
                    );
                } else {
                    info!(
                        self.ctx.expect_logger(),
                        "Resuming hord indexing from block #{}", start_block
                    );
                }

                crate::hord::perform_hord_db_update(
                    start_block,
                    end_block,
                    &self.config.get_hord_config(),
                    &self.config,
                    Some(tx.clone()),
                    &self.ctx,
                )
                .await?;
            }
        }

        // Stacks scan operation threadpool
        let (stacks_scan_op_tx, stacks_scan_op_rx) = crossbeam_channel::unbounded();
        let ctx = self.ctx.clone();
        let config = self.config.clone();
        let observer_command_tx_moved = observer_command_tx.clone();
        let _ = hiro_system_kit::thread_named("Stacks scan runloop")
            .spawn(move || {
                start_stacks_scan_runloop(
                    &config,
                    stacks_scan_op_rx,
                    observer_command_tx_moved,
                    &ctx,
                );
            })
            .expect("unable to spawn thread");

        // Bitcoin scan operation threadpool
        let (bitcoin_scan_op_tx, bitcoin_scan_op_rx) = crossbeam_channel::unbounded();
        let ctx = self.ctx.clone();
        let config = self.config.clone();
        let observer_command_tx_moved = observer_command_tx.clone();
        let _ = hiro_system_kit::thread_named("Bitcoin scan runloop")
            .spawn(move || {
                start_bitcoin_scan_runloop(
                    &config,
                    bitcoin_scan_op_rx,
                    observer_command_tx_moved,
                    &ctx,
                );
            })
            .expect("unable to spawn thread");

        // Enable HTTP Predicates API, if required
        if let PredicatesApi::On(ref api_config) = self.config.http_api {
            info!(
                self.ctx.expect_logger(),
                "Listening on port {} for chainhook predicate registrations", api_config.http_port
            );
            let ctx = self.ctx.clone();
            let api_config = api_config.clone();
            let moved_observer_command_tx = observer_command_tx.clone();
            // Test and initialize a database connection
            let _ = hiro_system_kit::thread_named("HTTP Predicate API").spawn(move || {
                let future = start_predicate_api_server(api_config, moved_observer_command_tx, ctx);
                let _ = hiro_system_kit::nestable_block_on(future);
            });
        }

        info!(
            self.ctx.expect_logger(),
            "Listening on port {} for Stacks chain events", event_observer_config.ingestion_port
        );
        match event_observer_config.bitcoin_block_signaling {
            BitcoinBlockSignaling::ZeroMQ(ref url) => {
                info!(
                    self.ctx.expect_logger(),
                    "Observing Bitcoin chain events via ZeroMQ: {}", url
                );
            }
            BitcoinBlockSignaling::Stacks(ref _url) => {
                // Start chainhook event observer
                let context_cloned = self.ctx.clone();
                let event_observer_config_moved = event_observer_config.clone();
                let observer_command_tx_moved = observer_command_tx.clone();
                let _ =
                    hiro_system_kit::thread_named("Chainhook event observer").spawn(move || {
                        let future = start_event_observer(
                            event_observer_config_moved,
                            observer_command_tx_moved,
                            observer_command_rx,
                            Some(observer_event_tx),
                            context_cloned,
                        );
                        let _ = hiro_system_kit::nestable_block_on(future);
                    });
                info!(
                    self.ctx.expect_logger(),
                    "Listening on port {} for Stacks chain events",
                    event_observer_config
                        .get_stacks_node_config()
                        .ingestion_port
                );
                info!(
                    self.ctx.expect_logger(),
                    "Observing Bitcoin chain events via Stacks node"
                );
            }
        }

        let mut stacks_event = 0;
        loop {
            let event = match observer_event_rx.recv() {
                Ok(cmd) => cmd,
                Err(e) => {
                    error!(
                        self.ctx.expect_logger(),
                        "Error: broken channel {}",
                        e.to_string()
                    );
                    break;
                }
            };
            match event {
                ObserverEvent::PredicateRegistered(spec) => {
                    // If start block specified, use it.
                    // If no start block specified, depending on the nature the hook, we'd like to retrieve:
                    // - contract-id
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let mut predicates_db_conn = match open_readwrite_predicates_db_conn(config)
                        {
                            Ok(con) => con,
                            Err(e) => {
                                error!(
                                    self.ctx.expect_logger(),
                                    "unable to register predicate: {}",
                                    e.to_string()
                                );
                                continue;
                            }
                        };
                        update_predicate_spec(
                            &spec.key(),
                            &spec,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                        update_predicate_status(
                            &spec.key(),
                            PredicateStatus::Disabled,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                    }
                    match spec {
                        ChainhookSpecification::Stacks(predicate_spec) => {
                            let _ = stacks_scan_op_tx.send(predicate_spec);
                        }
                        ChainhookSpecification::Bitcoin(predicate_spec) => {
                            let _ = bitcoin_scan_op_tx.send(predicate_spec);
                        }
                    }
                }
                ObserverEvent::PredicateEnabled(spec) => {
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let mut predicates_db_conn = match open_readwrite_predicates_db_conn(config)
                        {
                            Ok(con) => con,
                            Err(e) => {
                                error!(
                                    self.ctx.expect_logger(),
                                    "unable to enable predicate: {}",
                                    e.to_string()
                                );
                                continue;
                            }
                        };
                        update_predicate_spec(
                            &spec.key(),
                            &spec,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                        update_predicate_status(
                            &spec.key(),
                            PredicateStatus::InitialScanCompleted,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                    }
                }
                ObserverEvent::PredicateDeregistered(spec) => {
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let mut predicates_db_conn = match open_readwrite_predicates_db_conn(config)
                        {
                            Ok(con) => con,
                            Err(e) => {
                                error!(
                                    self.ctx.expect_logger(),
                                    "unable to deregister predicate: {}",
                                    e.to_string()
                                );
                                continue;
                            }
                        };
                        let predicate_key = spec.key();
                        let res: Result<(), redis::RedisError> =
                            predicates_db_conn.del(predicate_key);
                        if let Err(e) = res {
                            error!(
                                self.ctx.expect_logger(),
                                "unable to delete predicate: {}",
                                e.to_string()
                            );
                        }
                    }
                }
                ObserverEvent::BitcoinChainEvent((_chain_update, _report)) => {
                    debug!(self.ctx.expect_logger(), "Bitcoin update not stored");
                }
                ObserverEvent::StacksChainEvent((chain_event, report)) => {
                    let stacks_db_conn_rw = match open_readwrite_stacks_db_conn(
                        &self.config.expected_cache_path(),
                        &self.ctx,
                    ) {
                        Ok(db_conn) => db_conn,
                        Err(e) => {
                            error!(
                                self.ctx.expect_logger(),
                                "unable to store stacks block: {}",
                                e.to_string()
                            );
                            continue;
                        }
                    };
                    match &chain_event {
                        StacksChainEvent::ChainUpdatedWithBlocks(data) => {
                            stacks_event += 1;
                            confirm_entries_in_stacks_blocks(
                                &data.confirmed_blocks,
                                &stacks_db_conn_rw,
                                &self.ctx,
                            );
                            draft_entries_in_stacks_blocks(
                                &data.new_blocks,
                                &stacks_db_conn_rw,
                                &self.ctx,
                            )
                        }
                        StacksChainEvent::ChainUpdatedWithReorg(data) => {
                            confirm_entries_in_stacks_blocks(
                                &data.confirmed_blocks,
                                &stacks_db_conn_rw,
                                &self.ctx,
                            );
                            draft_entries_in_stacks_blocks(
                                &data.blocks_to_apply,
                                &stacks_db_conn_rw,
                                &self.ctx,
                            )
                        }
                        StacksChainEvent::ChainUpdatedWithMicroblocks(_)
                        | StacksChainEvent::ChainUpdatedWithMicroblocksReorg(_) => {}
                    };

                    for (predicate_uuid, blocks_ids) in report.predicates_evaluated.iter() {}

                    for (predicate_uuid, blocks_ids) in report.predicates_triggered.iter() {}
                    // Every 32 blocks, we will check if there's a new Stacks file archive to ingest
                    if stacks_event > 32 {
                        stacks_event = 0;
                        let _ = consolidate_local_stacks_chainstate_using_csv(
                            &mut self.config,
                            &self.ctx,
                        )
                        .await;
                    }
                }
                ObserverEvent::Terminate => {
                    info!(self.ctx.expect_logger(), "Terminating runloop");
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn replay_transfers(
        &self,
        start_block: u64,
        end_block: u64,
        block_post_processor: Option<Sender<BitcoinBlockData>>,
    ) -> Result<(), String> {
        info!(self.ctx.expect_logger(), "Transfers only");
        let inscriptions_db_conn =
            open_readonly_hord_db_conn(&self.config.expected_cache_path(), &self.ctx)?;

        info!(self.ctx.expect_logger(), "Fetching blocks");

        let bitcoin_config = BitcoinConfig {
            username: self.config.network.bitcoind_rpc_username.clone(),
            password: self.config.network.bitcoind_rpc_password.clone(),
            rpc_url: self.config.network.bitcoind_rpc_url.clone(),
            network: self.config.network.bitcoin_network.clone(),
            bitcoin_block_signaling: self.config.network.bitcoin_block_signaling.clone(),
        };
        let (tx, rx) = crossbeam_channel::bounded(1024);
        let moved_ctx = self.ctx.clone();
        hiro_system_kit::thread_named("Block fetch")
            .spawn(move || {
                for cursor in start_block..=end_block {
                    info!(moved_ctx.expect_logger(), "Fetching block {}", cursor);
                    let future = fetch_and_standardize_block(cursor, &bitcoin_config, &moved_ctx);

                    let block = hiro_system_kit::nestable_block_on(future).unwrap();

                    let _ = tx.send(block);
                }
            })
            .unwrap();

        let inscriptions_db_conn_rw =
            open_readwrite_hord_db_conn(&self.config.expected_cache_path(), &self.ctx)?;
        while let Ok(mut block) = rx.recv() {
            let network = match block.metadata.network {
                BitcoinNetwork::Mainnet => Network::Bitcoin,
                BitcoinNetwork::Regtest => Network::Regtest,
                BitcoinNetwork::Testnet => Network::Testnet,
            };

            let mut inscriptions_db_conn_rw =
                open_readwrite_hord_db_conn(&self.config.expected_cache_path(), &self.ctx)?;

            info!(
                self.ctx.expect_logger(),
                "Cleaning transfers from block {}", block.block_identifier.index
            );
            let inscriptions = find_all_inscriptions_in_block(
                &block.block_identifier.index,
                &inscriptions_db_conn,
                &self.ctx,
            );
            info!(
                self.ctx.expect_logger(),
                "{} inscriptions retrieved at block {}",
                inscriptions.len(),
                block.block_identifier.index
            );
            let mut operations = BTreeMap::new();

            {
                let transaction = inscriptions_db_conn_rw.transaction().unwrap();

                remove_entries_from_locations_at_block_height(
                    &block.block_identifier.index,
                    &transaction,
                    &self.ctx,
                );

                for (_, entry) in inscriptions.iter() {
                    let inscription_id = entry.get_inscription_id();
                    info!(
                        self.ctx.expect_logger(),
                        "Processing inscription {}", inscription_id
                    );
                    insert_entry_in_locations(
                        &inscription_id,
                        block.block_identifier.index,
                        &entry.transfer_data,
                        &transaction,
                        &self.ctx,
                    );

                    operations.insert(
                        entry.transaction_identifier_inscription.clone(),
                        OrdinalInscriptionTransferData {
                            inscription_id: entry.get_inscription_id(),
                            updated_address: None,
                            satpoint_pre_transfer: format_satpoint_to_watch(
                                &entry.transaction_identifier_inscription,
                                entry.inscription_input_index,
                                0,
                            ),
                            satpoint_post_transfer: format_satpoint_to_watch(
                                &entry.transfer_data.transaction_identifier_location,
                                entry.transfer_data.output_index,
                                entry.transfer_data.inscription_offset_intra_output,
                            ),
                            post_transfer_output_value: None,
                            tx_index: 0,
                            ordinal_number: Some(entry.ordinal_number),
                        },
                    );
                }
                transaction.commit().unwrap();
            }

            info!(
                self.ctx.expect_logger(),
                "Rewriting transfers for block {}", block.block_identifier.index
            );

            for (i, tx) in block.transactions.iter_mut().enumerate() {
                tx.metadata.ordinal_operations.clear();
                if let Some(mut entry) = operations.remove(&tx.transaction_identifier) {
                    let (_, output_index, _) =
                        parse_satpoint_to_watch(&entry.satpoint_post_transfer);

                    let script_pub_key_hex =
                        tx.metadata.outputs[output_index].get_script_pubkey_hex();
                    let updated_address = match Script::from_hex(&script_pub_key_hex) {
                        Ok(script) => match Address::from_script(&script, network.clone()) {
                            Ok(address) => Some(address.to_string()),
                            Err(e) => None,
                        },
                        Err(e) => None,
                    };

                    entry.updated_address = updated_address;
                    entry.post_transfer_output_value =
                        Some(tx.metadata.outputs[output_index].value);

                    tx.metadata
                        .ordinal_operations
                        .push(OrdinalOperation::InscriptionTransferred(entry));
                }
            }

            let mut storage = Storage::Sqlite(&inscriptions_db_conn_rw);
            update_storage_and_augment_bitcoin_block_with_inscription_transfer_data(
                &mut block,
                &mut storage,
                &self.ctx,
            )
            .unwrap();

            if let Some(ref tx) = block_post_processor {
                let _ = tx.send(block);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredicateStatus {
    Scanning(ScanningData),
    Streaming(StreamingData),
    InitialScanCompleted,
    Interrupted(String),
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanningData {
    pub start_block: u64,
    pub cursor: u64,
    pub end_block: u64,
    pub occurrences_found: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingData {
    pub last_occurence: u64,
    pub last_evaluation: u64,
}

pub fn update_predicate_status(
    predicate_key: &str,
    status: PredicateStatus,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let serialized_status = json!(status).to_string();
    if let Err(e) =
        predicates_db_conn.hset::<_, _, _, ()>(&predicate_key, "status", &serialized_status)
    {
        error!(
            ctx.expect_logger(),
            "Error updating status: {}",
            e.to_string()
        );
    } else {
        info!(
            ctx.expect_logger(),
            "Updating predicate {predicate_key} status: {serialized_status}"
        );
    }
}

pub fn update_predicate_spec(
    predicate_key: &str,
    spec: &ChainhookSpecification,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let serialized_spec = json!(spec).to_string();
    if let Err(e) =
        predicates_db_conn.hset::<_, _, _, ()>(&predicate_key, "specification", &serialized_spec)
    {
        error!(
            ctx.expect_logger(),
            "Error updating status: {}",
            e.to_string()
        );
    } else {
        info!(
            ctx.expect_logger(),
            "Updating predicate {predicate_key} with spec: {serialized_spec}"
        );
    }
}

pub fn retrieve_predicate_status(
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
) -> Option<PredicateStatus> {
    match predicates_db_conn.hget::<_, _, String>(predicate_key.to_string(), "status") {
        Ok(ref payload) => match serde_json::from_str(payload) {
            Ok(data) => Some(data),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

pub fn open_readwrite_predicates_db_conn(
    config: &PredicatesApiConfig,
) -> Result<Connection, String> {
    let redis_uri = &config.database_uri;
    let client = redis::Client::open(redis_uri.clone()).unwrap();
    client
        .get_connection()
        .map_err(|e| format!("unable to connect to db: {}", e.to_string()))
}

pub fn open_readwrite_predicates_db_conn_or_panic(
    config: &PredicatesApiConfig,
    ctx: &Context,
) -> Connection {
    let redis_con = match open_readwrite_predicates_db_conn(config) {
        Ok(con) => con,
        Err(message) => {
            error!(ctx.expect_logger(), "Redis: {}", message.to_string());
            panic!();
        }
    };
    redis_con
}
