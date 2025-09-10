pub(crate) mod http_api;
mod runloops;

use crate::config::{Config, PredicatesApi};
use crate::service::http_api::start_predicate_api_server;
use crate::service::runloops::{start_bitcoin_scan_runloop, start_stacks_scan_runloop};
use crate::storage::database_access::StacksDatabaseAccess;
use crate::storage::predicates_db::RedisPredicatesDatabaseAccess;
use crate::storage::signers::{initialize_signers_db, store_signer_db_messages};
use crate::storage::{
    confirm_entries_in_stacks_blocks, draft_entries_in_stacks_blocks, get_all_unconfirmed_blocks,
    get_last_block_height_inserted, open_readonly_stacks_db_conn_with_retry,
    open_readwrite_stacks_db_conn,
};

use chainhook_sdk::chainhooks::database::PredicatesDatabaseAccess;
use chainhook_sdk::chainhooks::types::{
    ChainhookSpecificationNetworkMap, PredicateStatus, ScanningData,
};

use chainhook_sdk::chainhooks::types::ChainhookInstance;
use chainhook_sdk::observer::{
    start_event_observer, ObserverCommand, ObserverEvent, PredicateDeregisteredEvent,
    StacksObserverStartupContext,
};
use chainhook_sdk::types::{Chain, StacksBlockData, StacksChainEvent};
use chainhook_sdk::utils::Context;
use chainhook_sdk::{try_error, try_info};

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use self::runloops::{BitcoinScanOp, StacksScanOp};

pub struct Service {
    config: Config,
    ctx: Context,
    stacks_block_processing_flag: Arc<AtomicBool>,
}

impl Service {
    pub fn new(
        config: Config,
        ctx: Context,
        stacks_block_processing_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config,
            ctx,
            stacks_block_processing_flag,
        }
    }

    pub async fn run(
        &mut self,
        predicates_from_startup: Vec<ChainhookSpecificationNetworkMap>,
        observer_commands_tx_rx: Option<(Sender<ObserverCommand>, Receiver<ObserverCommand>)>,
    ) -> Result<(), String> {
        // Create database access for the observer
        let stacks_database_access =
            StacksDatabaseAccess::new(PathBuf::from(&self.config.storage.working_dir));

        // Create Redis predicates database access
        let redis_uri = match &self.config.http_api {
            PredicatesApi::On(api_config) => api_config.database_uri.clone(),
            PredicatesApi::Off => panic!("Predicates API is not enabled"),
        };
        let predicates_database_access = RedisPredicatesDatabaseAccess::new(redis_uri);

        // store all predicates from Redis that were in the process of scanning when
        // chainhook was shutdown - we need to resume where we left off
        let mut leftover_scans = vec![];
        // retrieve predicates from Redis, and register each in memory
        if self.config.is_http_api_enabled() {
            let registered_predicates =
                match predicates_database_access.get_all_predicates(&self.ctx) {
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
            }
        }

        let mut newly_registered_predicates = vec![];
        for predicate in predicates_from_startup.into_iter() {
            let spec = match predicate {
                ChainhookSpecificationNetworkMap::Stacks(hook) => {
                    let spec =
                        hook.into_specification_for_network(&self.config.network.stacks_network)?;
                    ChainhookInstance::Stacks(spec)
                }
                ChainhookSpecificationNetworkMap::Bitcoin(hook) => {
                    let spec =
                        hook.into_specification_for_network(&self.config.network.bitcoin_network)?;
                    ChainhookInstance::Bitcoin(spec)
                }
            };
            newly_registered_predicates.push(spec);
        }

        initialize_signers_db(&self.config.expected_cache_path(), &self.ctx)
            .map_err(|e| format!("unable to initialize signers db: {e}"))?;

        let (observer_command_tx, observer_command_rx) =
            observer_commands_tx_rx.unwrap_or(channel());
        let (observer_event_tx, observer_event_rx) = crossbeam_channel::unbounded();

        let event_observer_config = self.config.get_event_observer_config();

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
            let moved_ctx = self.ctx.clone();
            let moved_api_config = api_config.clone();
            let moved_observer_command_tx = observer_command_tx.clone();
            let moved_predicates_database_access = predicates_database_access.clone();
            // Test and initialize a database connection
            let res = hiro_system_kit::thread_named("HTTP Predicate API")
                .spawn(move || {
                    let future = start_predicate_api_server(
                        moved_api_config,
                        moved_observer_command_tx,
                        moved_predicates_database_access,
                        moved_ctx,
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
            Some(stacks_startup_context),
            self.stacks_block_processing_flag.clone(),
            Some(stacks_database_access),
            predicates_database_access,
            self.ctx.clone(),
        );

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
                ObserverEvent::PredicateRegistered(spec) => match spec {
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
                },
                ObserverEvent::PredicateEnabled(_) => {}
                ObserverEvent::PredicateDeregistered(PredicateDeregisteredEvent {
                    predicate_uuid,
                    chain,
                }) => {
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
                }
                ObserverEvent::BitcoinChainEvent(_) => {}
                ObserverEvent::StacksChainEvent((chain_event, _)) => {
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
                            StacksChainEvent::ChainUpdatedWithNonConsensusEvents(data) => {
                                if let Err(e) = store_signer_db_messages(
                                    &self.config.expected_cache_path(),
                                    &data.events,
                                    &self.ctx,
                                ) {
                                    try_error!(self.ctx, "unable to store signer messages: {e}");
                                };
                                try_info!(
                                    self.ctx,
                                    "Stored {} stacks non-consensus events",
                                    data.events.len()
                                );
                            }
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
                    // Signal completion by setting block processing flag to false
                    self.stacks_block_processing_flag
                        .store(false, Ordering::Relaxed);
                }
                ObserverEvent::PredicateInterrupted(_) => {
                    // Stop sync?
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

#[cfg(test)]
pub mod tests;
