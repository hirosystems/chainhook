pub(crate) mod http_api;
mod runloops;

use crate::config::{Config, PredicatesApi, PredicatesApiConfig};
use crate::scan::stacks::consolidate_local_stacks_chainstate_using_csv;
use crate::service::http_api::{load_predicates_from_redis, start_predicate_api_server};
use crate::service::runloops::{start_bitcoin_scan_runloop, start_stacks_scan_runloop};
use crate::storage::{
    confirm_entries_in_stacks_blocks, draft_entries_in_stacks_blocks, get_all_unconfirmed_blocks,
    get_last_block_height_inserted, open_readonly_stacks_db_conn_with_retry,
    open_readwrite_stacks_db_conn,
};

use chainhook_sdk::chainhooks::types::{ChainhookSpecificationNetworkMap, ChainhookStore};

use chainhook_sdk::chainhooks::types::ChainhookInstance;
use chainhook_sdk::observer::{
    start_event_observer, HookExpirationData, ObserverCommand, ObserverEvent,
    PredicateDeregisteredEvent, PredicateEvaluationReport, PredicateInterruptedData,
    StacksObserverStartupContext,
};
use chainhook_sdk::types::{Chain, StacksBlockData, StacksChainEvent};
use chainhook_sdk::utils::Context;
use redis::{Commands, Connection};

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use self::http_api::get_entry_from_predicates_db;
use self::runloops::{BitcoinScanOp, StacksScanOp};

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
        predicates_from_startup: Vec<ChainhookSpecificationNetworkMap>,
        observer_commands_tx_rx: Option<(Sender<ObserverCommand>, Receiver<ObserverCommand>)>,
    ) -> Result<(), String> {
        let mut chainhook_store = ChainhookStore::new();

        // store all predicates from Redis that were in the process of scanning when
        // chainhook was shutdown - we need to resume where we left off
        let mut leftover_scans = vec![];
        // retrieve predicates from Redis, and register each in memory
        if self.config.is_http_api_enabled() {
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
            for (predicate, status) in registered_predicates.into_iter() {
                let predicate_uuid = predicate.uuid().to_string();
                match status {
                    PredicateStatus::Scanning(scanning_data) => {
                        leftover_scans.push((predicate.clone(), Some(scanning_data)));
                    }
                    PredicateStatus::New => {
                        leftover_scans.push((predicate.clone(), None));
                    }
                    // predicates that were previously in a streaming state probably
                    // need to catch up on blocks
                    PredicateStatus::Streaming(streaming_data) => {
                        let scanning_data = ScanningData {
                            number_of_blocks_to_scan: 0, // this is the only data we don't know when converting from streaming => scanning
                            number_of_blocks_evaluated: streaming_data.number_of_blocks_evaluated,
                            number_of_times_triggered: streaming_data.number_of_times_triggered,
                            last_occurrence: streaming_data.last_occurrence,
                            last_evaluated_block_height: streaming_data.last_evaluated_block_height,
                        };
                        leftover_scans.push((predicate.clone(), Some(scanning_data)));
                    }
                    PredicateStatus::UnconfirmedExpiration(_) => {}
                    PredicateStatus::ConfirmedExpiration(_) | PredicateStatus::Interrupted(_) => {
                        // Confirmed and Interrupted predicates don't need to be reregistered.
                        continue;
                    }
                }
                match chainhook_store.register_instance(predicate) {
                    Ok(_) => {
                        debug!(
                            self.ctx.expect_logger(),
                            "Predicate {} retrieved from storage and registered", predicate_uuid,
                        );
                    }
                    Err(e) => {
                        warn!(
                            self.ctx.expect_logger(),
                            "Failed to register predicate {} after retrieving from storage: {}",
                            predicate_uuid,
                            e.to_string()
                        );
                    }
                }
            }
        }

        let mut newly_registered_predicates = vec![];
        // For each predicate found, register in memory.
        for predicate in predicates_from_startup.into_iter() {
            if let PredicatesApi::On(api_config) = &self.config.http_api {
                if let Ok(mut predicates_db_conn) = open_readwrite_predicates_db_conn(api_config) {
                    let uuid = predicate.get_uuid();
                    if let Ok(Some(_)) = get_entry_from_predicates_db(
                        &ChainhookInstance::either_stx_or_btc_key(uuid),
                        &mut predicates_db_conn,
                        &self.ctx,
                    ) {
                        warn!(
                            self.ctx.expect_logger(),
                            "Predicate uuid already in use: {uuid}",
                        );
                        continue;
                    }
                };
            }
            match chainhook_store.register_instance_from_network_map(
                (
                    &self.config.network.bitcoin_network,
                    &self.config.network.stacks_network,
                ),
                predicate,
            ) {
                Ok(spec) => {
                    newly_registered_predicates.push(spec.clone());
                    debug!(
                        self.ctx.expect_logger(),
                        "Predicate {} retrieved from config and loaded",
                        spec.uuid(),
                    );
                }
                Err(e) => {
                    warn!(
                        self.ctx.expect_logger(),
                        "Failed to load predicate from config: {}",
                        e.to_string()
                    );
                }
            }
        }

        let (observer_command_tx, observer_command_rx) =
            observer_commands_tx_rx.unwrap_or(channel());
        let (observer_event_tx, observer_event_rx) = crossbeam_channel::unbounded();
        // let (ordinal_indexer_command_tx, ordinal_indexer_command_rx) = channel();

        let mut event_observer_config = self.config.get_event_observer_config();
        event_observer_config.registered_chainhooks = chainhook_store;

        // Download and ingest a Stacks dump
        if self.config.rely_on_remote_stacks_tsv() {
            consolidate_local_stacks_chainstate_using_csv(&mut self.config, &self.ctx).await?;
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
                    observer_command_tx_moved.clone(),
                    None,
                    &ctx,
                );
                // the scan runloop should loop forever; if it finishes, something is wrong
                crit!(ctx.expect_logger(), "Stacks scan runloop stopped.",);
                let _ = observer_command_tx_moved.send(ObserverCommand::Terminate);
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
                    observer_command_tx_moved.clone(),
                    None,
                    &ctx,
                );
                // the scan runloop should loop forever; if it finishes, something is wrong
                crit!(ctx.expect_logger(), "Bitcoin scan runloop stopped.",);
                let _ = observer_command_tx_moved.send(ObserverCommand::Terminate);
            })
            .expect("unable to spawn thread");

        // Enable HTTP Predicates API, if required
        let config = self.config.clone();
        let predicate_api_shutdown = if let PredicatesApi::On(ref api_config) = config.http_api {
            info!(
                self.ctx.expect_logger(),
                "Listening on port {} for chainhook predicate registrations", api_config.http_port
            );
            let ctx = self.ctx.clone();
            let api_config = api_config.clone();
            let moved_observer_command_tx = observer_command_tx.clone();
            // Test and initialize a database connection
            let res = hiro_system_kit::thread_named("HTTP Predicate API")
                .spawn(move || {
                    let future = start_predicate_api_server(
                        api_config,
                        moved_observer_command_tx.clone(),
                        ctx.clone(),
                    );
                    hiro_system_kit::nestable_block_on(future)
                })
                .expect("unable to spawn thread");
            let res = res.join().expect("unable to terminate thread");
            match res {
                Ok(predicate_api_shutdown) => Some(predicate_api_shutdown),
                Err(e) => {
                    return Err(format!(
                        "Predicate API Registration server failed to start: {}",
                        e
                    ));
                }
            }
        } else {
            None
        };

        let ctx = self.ctx.clone();
        let stacks_db =
            open_readonly_stacks_db_conn_with_retry(&config.expected_cache_path(), 3, &ctx)?;
        let confirmed_tip = get_last_block_height_inserted(&stacks_db, &ctx).unwrap_or(0);
        let stacks_startup_context = match get_all_unconfirmed_blocks(&stacks_db, &ctx) {
            Ok(blocks) => {
                // any unconfirmed blocks that are earlier than confirmed blocks are invalid

                let unconfirmed_blocks = blocks
                    .iter()
                    .filter(|&b| b.block_identifier.index > confirmed_tip)
                    .cloned()
                    .collect::<Vec<StacksBlockData>>();

                let highest_appended = match unconfirmed_blocks
                    .iter()
                    .max_by_key(|b| b.block_identifier.index)
                {
                    Some(highest_block) => highest_block.block_identifier.index,
                    None => confirmed_tip,
                };
                StacksObserverStartupContext {
                    block_pool_seed: unconfirmed_blocks,
                    last_block_height_appended: highest_appended,
                }
            }
            Err(e) => {
                info!(
                    self.ctx.expect_logger(),
                    "Failed to get stacks blocks from db to seed block pool: {}", e
                );
                StacksObserverStartupContext {
                    block_pool_seed: vec![],
                    last_block_height_appended: confirmed_tip,
                }
            }
        };

        let observer_event_tx_moved = observer_event_tx.clone();
        let moved_observer_command_tx = observer_command_tx.clone();
        let _ = start_event_observer(
            event_observer_config.clone(),
            moved_observer_command_tx,
            observer_command_rx,
            Some(observer_event_tx_moved),
            None,
            None,
            Some(stacks_startup_context),
            self.ctx.clone(),
        );

        let mut stacks_event = 0;

        let ctx = self.ctx.clone();
        match self.config.http_api {
            PredicatesApi::On(ref api_config) => {
                // Test redis connection
                open_readwrite_predicates_db_conn(api_config)?;
            }
            PredicatesApi::Off => {}
        };

        for predicate_with_last_scanned_block in leftover_scans {
            match predicate_with_last_scanned_block {
                (ChainhookInstance::Stacks(spec), last_scanned_block) => {
                    let _ = stacks_scan_op_tx.send(StacksScanOp::StartScan {
                        predicate_spec: spec,
                        unfinished_scan_data: last_scanned_block,
                    });
                }
                (ChainhookInstance::Bitcoin(spec), last_scanned_block) => {
                    let _ = bitcoin_scan_op_tx.send(BitcoinScanOp::StartScan {
                        predicate_spec: spec,
                        unfinished_scan_data: last_scanned_block,
                    });
                }
            }
        }

        for new_predicate in newly_registered_predicates {
            let _ = observer_event_tx.send(ObserverEvent::PredicateRegistered(new_predicate));
        }

        loop {
            let event = match observer_event_rx.recv() {
                Ok(cmd) => cmd,
                Err(e) => {
                    crit!(
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
                        let Ok(mut predicates_db_conn) =
                            open_readwrite_predicates_db_conn_verbose(config, &ctx)
                        else {
                            continue;
                        };
                        update_predicate_spec(
                            &spec.key(),
                            &spec,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                        update_predicate_status(
                            &spec.key(),
                            PredicateStatus::New,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                    }
                    match spec {
                        ChainhookInstance::Stacks(predicate_spec) => {
                            let _ = stacks_scan_op_tx.send(StacksScanOp::StartScan {
                                predicate_spec,
                                unfinished_scan_data: None,
                            });
                        }
                        ChainhookInstance::Bitcoin(predicate_spec) => {
                            let _ = bitcoin_scan_op_tx.send(BitcoinScanOp::StartScan {
                                predicate_spec,
                                unfinished_scan_data: None,
                            });
                        }
                    }
                }
                ObserverEvent::PredicateEnabled(spec) => {
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let Ok(mut predicates_db_conn) =
                            open_readwrite_predicates_db_conn_verbose(config, &ctx)
                        else {
                            continue;
                        };
                        update_predicate_spec(
                            &spec.key(),
                            &spec,
                            &mut predicates_db_conn,
                            &self.ctx,
                        );
                        set_predicate_streaming_status(
                            StreamingDataType::FinishedScanning,
                            &spec.key(),
                            &mut predicates_db_conn,
                            &ctx,
                        );
                    }
                }
                ObserverEvent::PredicateDeregistered(PredicateDeregisteredEvent {
                    predicate_uuid,
                    chain,
                }) => {
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let Ok(mut predicates_db_conn) =
                            open_readwrite_predicates_db_conn_verbose(config, &ctx)
                        else {
                            continue;
                        };

                        match chain {
                            Chain::Bitcoin => {
                                let _ = bitcoin_scan_op_tx
                                    .send(BitcoinScanOp::KillScan(predicate_uuid.clone()));
                            }
                            Chain::Stacks => {
                                let _ = stacks_scan_op_tx
                                    .send(StacksScanOp::KillScan(predicate_uuid.clone()));
                            }
                        };

                        let predicate_key =
                            ChainhookInstance::either_stx_or_btc_key(&predicate_uuid);
                        let res: Result<(), redis::RedisError> =
                            predicates_db_conn.del(predicate_key.clone());
                        if let Err(e) = res {
                            warn!(
                                self.ctx.expect_logger(),
                                "unable to delete predicate {predicate_key}: {}",
                                e.to_string()
                            );
                        }
                    }
                }
                ObserverEvent::BitcoinChainEvent((chain_update, report)) => {
                    debug!(self.ctx.expect_logger(), "Bitcoin update not stored");
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let Ok(mut predicates_db_conn) =
                            open_readwrite_predicates_db_conn_verbose(config, &ctx)
                        else {
                            continue;
                        };

                        match chain_update {
                            chainhook_sdk::types::BitcoinChainEvent::ChainUpdatedWithBlocks(
                                data,
                            ) => {
                                for confirmed_block in &data.confirmed_blocks {
                                    if let Some(expired_predicate_uuids) = expire_predicates_for_block(
                                        &Chain::Bitcoin,
                                        confirmed_block.block_identifier.index,
                                        &mut predicates_db_conn,
                                        &ctx,
                                    ) {
                                        for uuid in expired_predicate_uuids.into_iter() {
                                            let _ = observer_command_tx.send(
                                                ObserverCommand::ExpireBitcoinPredicate(
                                                    HookExpirationData {
                                                        hook_uuid: uuid,
                                                        block_height: confirmed_block
                                                            .block_identifier
                                                            .index,
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                            chainhook_sdk::types::BitcoinChainEvent::ChainUpdatedWithReorg(
                                data,
                            ) => {
                                for confirmed_block in &data.confirmed_blocks {
                                    if let Some(expired_predicate_uuids) = expire_predicates_for_block(
                                        &Chain::Bitcoin,
                                        confirmed_block.block_identifier.index,
                                        &mut predicates_db_conn,
                                        &ctx,
                                    ) {
                                        for uuid in expired_predicate_uuids.into_iter() {
                                            let _ = observer_command_tx.send(
                                                ObserverCommand::ExpireBitcoinPredicate(
                                                    HookExpirationData {
                                                        hook_uuid: uuid,
                                                        block_height: confirmed_block
                                                            .block_identifier
                                                            .index,
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        update_status_from_report(
                            Chain::Bitcoin,
                            report,
                            &mut predicates_db_conn,
                            &ctx,
                        );
                    }
                }
                ObserverEvent::StacksChainEvent((chain_event, report)) => {
                    match open_readwrite_stacks_db_conn(
                        &self.config.expected_cache_path(),
                        &self.ctx,
                    ) {
                        Ok(stacks_db_conn_rw) => match &chain_event {
                            StacksChainEvent::ChainUpdatedWithBlocks(data) => {
                                if let Err(e) = confirm_entries_in_stacks_blocks(
                                    &data.confirmed_blocks,
                                    &stacks_db_conn_rw,
                                    &self.ctx,
                                ) {
                                    error!(
                                        self.ctx.expect_logger(),
                                        "unable to add confirmed entries to stacks db: {}", e
                                    );
                                };
                                if let Err(e) = draft_entries_in_stacks_blocks(
                                    &data.new_blocks,
                                    &stacks_db_conn_rw,
                                    &self.ctx,
                                ) {
                                    error!(
                                        self.ctx.expect_logger(),
                                        "unable to add unconfirmed entries to stacks db: {}", e
                                    );
                                };
                            }
                            StacksChainEvent::ChainUpdatedWithReorg(data) => {
                                if let Err(e) = confirm_entries_in_stacks_blocks(
                                    &data.confirmed_blocks,
                                    &stacks_db_conn_rw,
                                    &self.ctx,
                                ) {
                                    error!(
                                        self.ctx.expect_logger(),
                                        "unable to add confirmed entries to stacks db: {}", e
                                    );
                                };
                                if let Err(e) = draft_entries_in_stacks_blocks(
                                    &data.blocks_to_apply,
                                    &stacks_db_conn_rw,
                                    &self.ctx,
                                ) {
                                    error!(
                                        self.ctx.expect_logger(),
                                        "unable to add unconfirmed entries to stacks db: {}", e
                                    );
                                };
                            }
                            StacksChainEvent::ChainUpdatedWithMicroblocks(_)
                            | StacksChainEvent::ChainUpdatedWithMicroblocksReorg(_) => {}
                        },
                        Err(e) => {
                            error!(
                                self.ctx.expect_logger(),
                                "unable to open stacks db: {}",
                                e.to_string()
                            );
                            continue;
                        }
                    };

                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let Ok(mut predicates_db_conn) =
                            open_readwrite_predicates_db_conn_verbose(config, &ctx)
                        else {
                            continue;
                        };

                        match &chain_event {
                            StacksChainEvent::ChainUpdatedWithBlocks(data) => {
                                stacks_event += 1;
                                for confirmed_block in &data.confirmed_blocks {
                                    if let Some(expired_predicate_uuids) = expire_predicates_for_block(
                                        &Chain::Stacks,
                                        confirmed_block.block_identifier.index,
                                        &mut predicates_db_conn,
                                        &ctx,
                                    ) {
                                        for uuid in expired_predicate_uuids.into_iter() {
                                            let _ = observer_command_tx.send(
                                                ObserverCommand::ExpireStacksPredicate(
                                                    HookExpirationData {
                                                        hook_uuid: uuid,
                                                        block_height: confirmed_block
                                                            .block_identifier
                                                            .index,
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                            StacksChainEvent::ChainUpdatedWithReorg(data) => {
                                for confirmed_block in &data.confirmed_blocks {
                                    if let Some(expired_predicate_uuids) = expire_predicates_for_block(
                                        &Chain::Stacks,
                                        confirmed_block.block_identifier.index,
                                        &mut predicates_db_conn,
                                        &ctx,
                                    ) {
                                        for uuid in expired_predicate_uuids.into_iter() {
                                            let _ = observer_command_tx.send(
                                                ObserverCommand::ExpireStacksPredicate(
                                                    HookExpirationData {
                                                        hook_uuid: uuid,
                                                        block_height: confirmed_block
                                                            .block_identifier
                                                            .index,
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                            StacksChainEvent::ChainUpdatedWithMicroblocks(_)
                            | StacksChainEvent::ChainUpdatedWithMicroblocksReorg(_) => {}
                        };
                        update_status_from_report(
                            Chain::Stacks,
                            report,
                            &mut predicates_db_conn,
                            &ctx,
                        );
                    };

                    // Every 32 blocks, we will check if there's a new Stacks file archive to ingest
                    if stacks_event > 32 {
                        stacks_event = 0;
                        if self.config.rely_on_remote_stacks_tsv() {
                            if let Err(e) = consolidate_local_stacks_chainstate_using_csv(
                                &mut self.config,
                                &self.ctx,
                            )
                            .await {
                                error!(
                                    self.ctx.expect_logger(),
                                    "Failed to update database from archive: {e}"
                                )
                            };
                        }
                    }
                }
                ObserverEvent::PredicateInterrupted(PredicateInterruptedData {
                    predicate_key,
                    error,
                }) => {
                    if let PredicatesApi::On(ref config) = self.config.http_api {
                        let Ok(mut predicates_db_conn) =
                            open_readwrite_predicates_db_conn_verbose(config, &ctx)
                        else {
                            continue;
                        };
                        set_predicate_interrupted_status(
                            error,
                            &predicate_key,
                            &mut predicates_db_conn,
                            &ctx,
                        );
                    }
                }
                ObserverEvent::Terminate => {
                    info!(
                        self.ctx.expect_logger(),
                        "Terminating ObserverEvent runloop"
                    );
                    if let Some(predicate_api_shutdown) = predicate_api_shutdown {
                        info!(
                            self.ctx.expect_logger(),
                            "Terminating Predicate Registration API"
                        );
                        predicate_api_shutdown.notify();
                    }
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type", content = "info")]
/// A high-level view of how `PredicateStatus` is used/updated can be seen here: docs/images/predicate-status-flowchart/PredicateStatusFlowchart.png.
pub enum PredicateStatus {
    Scanning(ScanningData),
    Streaming(StreamingData),
    UnconfirmedExpiration(ExpiredData),
    ConfirmedExpiration(ExpiredData),
    Interrupted(String),
    New,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ScanningData {
    pub number_of_blocks_to_scan: u64,
    pub number_of_blocks_evaluated: u64,
    pub number_of_times_triggered: u64,
    pub last_occurrence: Option<u64>,
    pub last_evaluated_block_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamingData {
    pub last_occurrence: Option<u64>,
    pub last_evaluation: u64,
    pub number_of_times_triggered: u64,
    pub number_of_blocks_evaluated: u64,
    pub last_evaluated_block_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpiredData {
    pub number_of_blocks_evaluated: u64,
    pub number_of_times_triggered: u64,
    pub last_occurrence: Option<u64>,
    pub last_evaluated_block_height: u64,
    pub expired_at_block_height: u64,
}

fn update_status_from_report(
    chain: Chain,
    report: PredicateEvaluationReport,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    for (predicate_uuid, blocks_ids) in report.predicates_triggered.iter() {
        if let Some(last_triggered_height) = blocks_ids.last().map(|b| b.index) {
            let triggered_count = blocks_ids.len().try_into().unwrap_or(0);
            set_predicate_streaming_status(
                StreamingDataType::Occurrence {
                    last_triggered_height,
                    triggered_count,
                },
                &(ChainhookInstance::either_stx_or_btc_key(predicate_uuid)),
                predicates_db_conn,
                ctx,
            );
        }
    }

    for (predicate_uuid, blocks_ids) in report.predicates_evaluated.iter() {
        // clone so we don't actually update the report
        let mut blocks_ids = blocks_ids.clone();
        // any triggered or expired predicate was also evaluated. But we already updated the status for that block,
        // so remove those matching blocks from the list of evaluated predicates
        if let Some(triggered_block_ids) = report.predicates_triggered.get(predicate_uuid) {
            for triggered_id in triggered_block_ids {
                blocks_ids.remove(triggered_id);
            }
        }
        if let Some(expired_block_ids) = report.predicates_expired.get(predicate_uuid) {
            for expired_id in expired_block_ids {
                blocks_ids.remove(expired_id);
            }
        }
        if let Some(last_evaluated_height) = blocks_ids.last().map(|b| b.index) {
            let evaluated_count = blocks_ids.len().try_into().unwrap_or(0);
            set_predicate_streaming_status(
                StreamingDataType::Evaluation {
                    last_evaluated_height,
                    evaluated_count,
                },
                &(ChainhookInstance::either_stx_or_btc_key(predicate_uuid)),
                predicates_db_conn,
                ctx,
            );
        }
    }
    for (predicate_uuid, blocks_ids) in report.predicates_expired.iter() {
        if let Some(last_evaluated_height) = blocks_ids.last().map(|b| b.index) {
            let evaluated_count = blocks_ids.len().try_into().unwrap_or(0);
            set_unconfirmed_expiration_status(
                &chain,
                evaluated_count,
                last_evaluated_height,
                &(ChainhookInstance::either_stx_or_btc_key(predicate_uuid)),
                predicates_db_conn,
                ctx,
            );
        }
    }
}

fn set_predicate_interrupted_status(
    error: String,
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let status = PredicateStatus::Interrupted(error);
    update_predicate_status(predicate_key, status, predicates_db_conn, ctx);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingDataType {
    Occurrence {
        last_triggered_height: u64,
        triggered_count: u64,
    },
    Evaluation {
        last_evaluated_height: u64,
        evaluated_count: u64,
    },
    FinishedScanning,
}

/// Updates a predicate's status to `Streaming` if `Scanning` is complete.
///
/// If `StreamingStatusType` is `Occurrence`, sets the `last_occurrence` & `last_evaluation` fields to the current time.
///
/// If `StreamingStatusType` is `Evaluation`, sets the `last_evaluation` field to the current time while leaving the `last_occurrence` field as it was.
fn set_predicate_streaming_status(
    streaming_data_type: StreamingDataType,
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Could not get current time in ms")
        .as_secs();
    let (
        last_occurrence,
        number_of_blocks_evaluated,
        number_of_times_triggered,
        last_evaluated_block_height,
    ) = {
        let current_status = retrieve_predicate_status(predicate_key, predicates_db_conn);
        match current_status {
            Some(status) => match status {
                PredicateStatus::Streaming(StreamingData {
                    last_occurrence,
                    number_of_blocks_evaluated,
                    number_of_times_triggered,
                    last_evaluated_block_height,
                    last_evaluation: _,
                }) => (
                    last_occurrence,
                    number_of_blocks_evaluated,
                    number_of_times_triggered,
                    last_evaluated_block_height,
                ),
                PredicateStatus::Scanning(ScanningData {
                    number_of_blocks_to_scan: _,
                    number_of_blocks_evaluated,
                    number_of_times_triggered,
                    last_evaluated_block_height,
                    last_occurrence,
                }) => (
                    last_occurrence,
                    number_of_blocks_evaluated,
                    number_of_times_triggered,
                    last_evaluated_block_height,
                ),
                PredicateStatus::UnconfirmedExpiration(ExpiredData {
                    number_of_blocks_evaluated,
                    number_of_times_triggered,
                    last_occurrence,
                    last_evaluated_block_height,
                    expired_at_block_height: _,
                }) => (
                    last_occurrence,
                    number_of_blocks_evaluated,
                    number_of_times_triggered,
                    last_evaluated_block_height,
                ),
                PredicateStatus::New => (None, 0, 0, 0),
                PredicateStatus::Interrupted(_) | PredicateStatus::ConfirmedExpiration(_) => {
                    warn!(ctx.expect_logger(), "Attempting to set Streaming status when previous status was {:?} for predicate {}", status, predicate_key);
                    return;
                }
            },
            None => (None, 0, 0, 0),
        }
    };
    let (
        last_occurrence,
        number_of_times_triggered,
        number_of_blocks_evaluated,
        last_evaluated_block_height,
    ) = match streaming_data_type {
        StreamingDataType::Occurrence {
            last_triggered_height,
            triggered_count,
        } => (
            Some(now_secs),
            number_of_times_triggered + triggered_count,
            number_of_blocks_evaluated + triggered_count,
            last_triggered_height,
        ),
        StreamingDataType::Evaluation {
            last_evaluated_height,
            evaluated_count,
        } => (
            last_occurrence,
            number_of_times_triggered,
            number_of_blocks_evaluated + evaluated_count,
            last_evaluated_height,
        ),
        StreamingDataType::FinishedScanning => (
            last_occurrence,
            number_of_times_triggered,
            number_of_blocks_evaluated,
            last_evaluated_block_height,
        ),
    };

    update_predicate_status(
        predicate_key,
        PredicateStatus::Streaming(StreamingData {
            last_occurrence,
            last_evaluation: now_secs,
            number_of_times_triggered,
            last_evaluated_block_height,
            number_of_blocks_evaluated,
        }),
        predicates_db_conn,
        ctx,
    );
}

/// Updates a predicate's status to `Scanning`.
///
/// Sets the `last_occurrence` time to the current time if a new trigger has occurred since the last status update.
pub fn set_predicate_scanning_status(
    predicate_key: &str,
    number_of_blocks_to_scan: u64,
    number_of_blocks_evaluated: u64,
    number_of_times_triggered: u64,
    current_block_height: u64,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Could not get current time in ms")
        .as_secs();
    let current_status = retrieve_predicate_status(predicate_key, predicates_db_conn);
    let last_occurrence = match current_status {
        Some(status) => match status {
            PredicateStatus::Scanning(scanning_data) => {
                if number_of_times_triggered > scanning_data.number_of_times_triggered {
                    Some(now_secs)
                } else {
                    scanning_data.last_occurrence
                }
            }
            PredicateStatus::Streaming(streaming_data) => {
                if number_of_times_triggered > streaming_data.number_of_times_triggered {
                    Some(now_secs)
                } else {
                    streaming_data.last_occurrence
                }
            }
            PredicateStatus::UnconfirmedExpiration(expired_data) => {
                if number_of_times_triggered > expired_data.number_of_times_triggered {
                    Some(now_secs)
                } else {
                    expired_data.last_occurrence
                }
            }
            PredicateStatus::New => {
                if number_of_times_triggered > 0 {
                    Some(now_secs)
                } else {
                    None
                }
            }
            PredicateStatus::ConfirmedExpiration(_) | PredicateStatus::Interrupted(_) => {
                warn!(ctx.expect_logger(), "Attempting to set Scanning status when previous status was {:?} for predicate {}", status, predicate_key);
                return;
            }
        },
        None => None,
    };

    update_predicate_status(
        predicate_key,
        PredicateStatus::Scanning(ScanningData {
            number_of_blocks_to_scan,
            number_of_blocks_evaluated,
            number_of_times_triggered,
            last_occurrence,
            last_evaluated_block_height: current_block_height,
        }),
        predicates_db_conn,
        ctx,
    );
}

/// Updates a predicate's status to `UnconfirmedExpiration`.
pub fn set_unconfirmed_expiration_status(
    chain: &Chain,
    number_of_new_blocks_evaluated: u64,
    last_evaluated_block_height: u64,
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let current_status = retrieve_predicate_status(predicate_key, predicates_db_conn);
    let mut previously_was_unconfirmed = false;
    let (
        number_of_blocks_evaluated,
        number_of_times_triggered,
        last_occurrence,
        expired_at_block_height,
    ) = match current_status {
        Some(status) => match status {
            PredicateStatus::Scanning(ScanningData {
                number_of_blocks_to_scan: _,
                number_of_blocks_evaluated: _,
                number_of_times_triggered,
                last_occurrence,
                last_evaluated_block_height,
            }) => (
                number_of_new_blocks_evaluated,
                number_of_times_triggered,
                last_occurrence,
                last_evaluated_block_height,
            ),
            PredicateStatus::New => (0, 0, None, 0),
            PredicateStatus::Streaming(StreamingData {
                last_occurrence,
                last_evaluation: _,
                number_of_times_triggered,
                number_of_blocks_evaluated,
                last_evaluated_block_height,
            }) => (
                number_of_blocks_evaluated + number_of_new_blocks_evaluated,
                number_of_times_triggered,
                last_occurrence,
                last_evaluated_block_height,
            ),
            PredicateStatus::UnconfirmedExpiration(ExpiredData {
                number_of_blocks_evaluated,
                number_of_times_triggered,
                last_occurrence,
                last_evaluated_block_height: _,
                expired_at_block_height,
            }) => {
                previously_was_unconfirmed = true;
                (
                    number_of_blocks_evaluated + number_of_new_blocks_evaluated,
                    number_of_times_triggered,
                    last_occurrence,
                    expired_at_block_height,
                )
            }
            PredicateStatus::ConfirmedExpiration(_) | PredicateStatus::Interrupted(_) => {
                warn!(ctx.expect_logger(), "Attempting to set UnconfirmedExpiration status when previous status was {:?} for predicate {}", status, predicate_key);
                return;
            }
        },
        None => (0, 0, None, 0),
    };
    update_predicate_status(
        predicate_key,
        PredicateStatus::UnconfirmedExpiration(ExpiredData {
            number_of_blocks_evaluated,
            number_of_times_triggered,
            last_occurrence,
            last_evaluated_block_height,
            expired_at_block_height,
        }),
        predicates_db_conn,
        ctx,
    );
    // don't insert this entry more than once
    if !previously_was_unconfirmed {
        insert_predicate_expiration(
            chain,
            expired_at_block_height,
            predicate_key,
            predicates_db_conn,
            ctx,
        );
    }
}

pub fn set_confirmed_expiration_status(
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let current_status = retrieve_predicate_status(predicate_key, predicates_db_conn);
    let expired_data = match current_status {
        Some(status) => match status {
            PredicateStatus::UnconfirmedExpiration(expired_data) => expired_data,
            PredicateStatus::ConfirmedExpiration(_)
            | PredicateStatus::Interrupted(_)
            | PredicateStatus::New
            | PredicateStatus::Scanning(_)
            | PredicateStatus::Streaming(_) => {
                warn!(ctx.expect_logger(), "Attempting to set ConfirmedExpiration status when previous status was {:?} for predicate {}", status, predicate_key);
                return;
            }
        },
        None => {
            // None means the predicate was deleted, so we can just ignore this predicate expiring
            return;
        }
    };
    update_predicate_status(
        predicate_key,
        PredicateStatus::ConfirmedExpiration(expired_data),
        predicates_db_conn,
        ctx,
    );
}

fn get_predicate_expiration_key(chain: &Chain, block_height: u64) -> String {
    match chain {
        Chain::Bitcoin => format!("expires_at:bitcoin_block:{}", block_height),
        Chain::Stacks => format!("expires_at:stacks_block:{}", block_height),
    }
}
fn expire_predicates_for_block(
    chain: &Chain,
    confirmed_block_index: u64,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) -> Option<Vec<String>> {
    match get_predicates_expiring_at_block(chain, confirmed_block_index, predicates_db_conn, ctx) {
        Some(predicates_to_expire) => {
            for predicate_key in predicates_to_expire.iter() {
                set_confirmed_expiration_status(predicate_key, predicates_db_conn, ctx);
            }
            Some(predicates_to_expire)
        }
        None => None,
    }
}

fn insert_predicate_expiration(
    chain: &Chain,
    expired_at_block_height: u64,
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let key = get_predicate_expiration_key(chain, expired_at_block_height);
    let mut predicates_expiring_at_block =
        get_predicates_expiring_at_block(chain, expired_at_block_height, predicates_db_conn, ctx)
            .unwrap_or_default();
    predicates_expiring_at_block.push(predicate_key.to_owned());
    let serialized_expiring_predicates = json!(predicates_expiring_at_block).to_string();
    if let Err(e) =
        predicates_db_conn.hset::<_, _, _, ()>(&key, "predicates", &serialized_expiring_predicates)
    {
        warn!(
            ctx.expect_logger(),
            "Error updating expired predicates index: {}",
            e.to_string()
        );
    } else {
        debug!(
            ctx.expect_logger(),
            "Updating expired predicates at block height {expired_at_block_height} with predicate: {predicate_key}"
        );
    }
}

fn get_predicates_expiring_at_block(
    chain: &Chain,
    block_index: u64,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) -> Option<Vec<String>> {
    let key = get_predicate_expiration_key(chain, block_index);
    match predicates_db_conn.hget::<_, _, String>(key.to_string(), "predicates") {
        Ok(ref payload) => match serde_json::from_str(payload) {
            Ok(data) => {
                if let Err(e) = predicates_db_conn.hdel::<_, _, u64>(key.to_string(), "predicates")
                {
                    warn!(
                        ctx.expect_logger(),
                        "Error removing expired predicates index: {}",
                        e.to_string()
                    );
                }
                Some(data)
            }
            Err(_) => None,
        },
        Err(_) => None,
    }
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
        warn!(
            ctx.expect_logger(),
            "Error updating status for {}: {}",
            predicate_key,
            e.to_string()
        );
    } else {
        debug!(
            ctx.expect_logger(),
            "Updating predicate {predicate_key} status: {serialized_status}"
        );
    }
}

fn update_predicate_spec(
    predicate_key: &str,
    spec: &ChainhookInstance,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let serialized_spec = json!(spec).to_string();
    if let Err(e) =
        predicates_db_conn.hset::<_, _, _, ()>(&predicate_key, "specification", &serialized_spec)
    {
        warn!(
            ctx.expect_logger(),
            "Error updating status for {}: {}",
            predicate_key,
            e.to_string()
        );
    } else {
        debug!(
            ctx.expect_logger(),
            "Updating predicate {predicate_key} with spec: {serialized_spec}"
        );
    }
}

fn retrieve_predicate_status(
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
        .map_err(|e| format!("unable to connect to db: {}", e))
}

pub fn open_readwrite_predicates_db_conn_verbose(
    config: &PredicatesApiConfig,
    ctx: &Context,
) -> Result<Connection, String> {
    let res = open_readwrite_predicates_db_conn(config);
    if let Err(ref e) = res {
        error!(ctx.expect_logger(), "{}", e.to_string());
    }
    res
}

// todo: evaluate expects
pub fn open_readwrite_predicates_db_conn_or_panic(
    config: &PredicatesApiConfig,
    ctx: &Context,
) -> Connection {
    open_readwrite_predicates_db_conn_verbose(config, ctx).expect("unable to open redis conn")
}

#[cfg(test)]
pub mod tests;
