pub(crate) mod http_api;
mod runloops;

use crate::config::{Config, PredicatesApi, PredicatesApiConfig};
use crate::scan::stacks::consolidate_local_stacks_chainstate_using_csv;
use crate::service::http_api::{load_predicates_from_redis, start_predicate_api_server};
use crate::service::runloops::{start_bitcoin_scan_runloop, start_stacks_scan_runloop};
use crate::storage::{
    confirm_entries_in_stacks_blocks, draft_entries_in_stacks_blocks, open_readwrite_stacks_db_conn,
};

use chainhook_sdk::chainhooks::types::{ChainhookConfig, ChainhookFullSpecification};

use chainhook_sdk::chainhooks::types::ChainhookSpecification;
use chainhook_sdk::observer::{start_event_observer, ObserverEvent};
use chainhook_sdk::types::StacksChainEvent;
use chainhook_sdk::utils::Context;
use redis::{Commands, Connection};

use std::sync::mpsc::channel;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Service {
    config: Config,
    ctx: Context,
}

impl Service {
    pub fn new(config: Config, ctx: Context) -> Self {
        Self { config, ctx }
    }

    pub async fn run(&mut self, predicates: Vec<ChainhookFullSpecification>) -> Result<(), String> {
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

        // Download and ingest a Stacks dump
        if self.config.rely_on_remote_stacks_tsv() {
            let _ =
                consolidate_local_stacks_chainstate_using_csv(&mut self.config, &self.ctx).await;
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
        let config = self.config.clone();
        if let PredicatesApi::On(ref api_config) = config.http_api {
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

        let _ = start_event_observer(
            event_observer_config.clone(),
            observer_command_tx,
            observer_command_rx,
            Some(observer_event_tx),
            None,
            self.ctx.clone(),
        );

        let mut stacks_event = 0;

        let ctx = self.ctx.clone();
        let mut predicates_db_conn = match self.config.http_api {
            PredicatesApi::On(ref api_config) => {
                Some(open_readwrite_predicates_db_conn_or_panic(api_config, &ctx))
            }
            PredicatesApi::Off => None,
        };

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
                        set_initial_scan_complete_status(&spec.key(), &mut predicates_db_conn, &ctx)
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
                    if let Some(ref mut predicates_db_conn) = predicates_db_conn {
                        for (predicate_uuid, _blocks_ids) in report.predicates_evaluated.iter() {
                            set_predicate_streaming_status(
                                StreamingDataType::Evaluation,
                                &(ChainhookSpecification::stacks_key(predicate_uuid)),
                                predicates_db_conn,
                                &ctx,
                            );
                        }

                        for (predicate_uuid, _blocks_ids) in report.predicates_triggered.iter() {
                            set_predicate_streaming_status(
                                StreamingDataType::Occurrence,
                                &(ChainhookSpecification::stacks_key(predicate_uuid)),
                                predicates_db_conn,
                                &ctx,
                            );
                        }
                    }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredicateStatus {
    Scanning(ScanningData),
    Streaming(StreamingData),
    InitialScanCompleted(CompletedScanData),
    Interrupted(String),
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanningData {
    pub number_of_blocks_to_scan: u64,
    pub number_of_blocks_scanned: u64,
    pub number_of_times_triggered: u64,
    pub last_occurrence: u128,
    pub current_block_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingData {
    pub last_occurrence: u128,
    pub last_evaluation: u128,
    pub number_of_times_triggered: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedScanData {
    pub number_of_blocks_scanned: u64,
    pub number_of_times_triggered: u64,
    pub last_occurrence: u128,
    pub final_block_height_scanned: u64,
}

// todo: consider using this so we can maintain number of times triggered after an interruption.
// what i don't know is - can we continue after an interruption to get further status updates?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptedData {
    error: String,
    pub number_of_times_triggered: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingDataType {
    Occurrence,
    Evaluation,
}

/// Updates a predicates status to `Streaming` if `Scanning` is complete.
///
/// If `StreamingStatusType` is `Occurrence`, sets the `last_occurrence` field to the current time while leaving the `last_evaluation` field as it was.
///
/// If `StreamingStatusType` is `Evaluation`, sets the `last_evaluation` field to the current time while leaving the `last_occurrence` field as it was.
fn set_predicate_streaming_status(
    streaming_data_type: StreamingDataType,
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Could not get current time in ms")
        .as_millis();
    let (mut last_occurrence, number_of_times_triggered) = {
        let current_status = retrieve_predicate_status(&predicate_key, predicates_db_conn);
        match current_status {
            Some(status) => match status {
                PredicateStatus::Streaming(streaming_data) => (
                    streaming_data.last_occurrence,
                    streaming_data.number_of_times_triggered,
                ),
                PredicateStatus::InitialScanCompleted(completed_data) => (
                    completed_data.last_occurrence,
                    completed_data.number_of_times_triggered,
                ),
                // here, if we have a status of "interrupted", we assume that the scan has completed.
                // the downside of this assumption is that _if_ scanning actually completed, we're
                // going to write an incorrect status of "streaming". However, since scanning is likely
                // to once again overwrite this status soon when another block is scanned, it's really nbd
                PredicateStatus::Interrupted(_) => (0, 0),
                PredicateStatus::Scanning(_) | PredicateStatus::Disabled => {
                    unreachable!("unreachable predicate status: {:?}", status)
                }
            },
            None => (0, 0),
        }
    };
    match streaming_data_type {
        StreamingDataType::Occurrence => last_occurrence = now_ms,
        _ => {}
    }

    update_predicate_status(
        predicate_key,
        PredicateStatus::Streaming(StreamingData {
            last_occurrence,
            last_evaluation: now_ms,
            number_of_times_triggered,
        }),
        predicates_db_conn,
        &ctx,
    );
}

pub fn set_predicate_scanning_status(
    predicate_key: &str,
    number_of_blocks_to_scan: u64,
    number_of_blocks_scanned: u64,
    number_of_times_triggered: u64,
    current_block_height: u64,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    info!(ctx.expect_logger(), "Setting scan status");
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Could not get current time in ms")
        .as_millis();
    let current_status = retrieve_predicate_status(&predicate_key, predicates_db_conn);
    let last_occurrence = match current_status {
        Some(status) => match status {
            PredicateStatus::Scanning(scanning_data) => {
                if number_of_times_triggered > scanning_data.number_of_times_triggered {
                    now_ms
                } else {
                    scanning_data.last_occurrence
                }
            }
            PredicateStatus::Interrupted(_) | PredicateStatus::Disabled => {
                if number_of_times_triggered > 0 {
                    now_ms
                } else {
                    0
                }
            }
            PredicateStatus::Streaming(_) | PredicateStatus::InitialScanCompleted(_) => {
                unreachable!("unreachable predicate status: {:?}", status)
            }
        },
        None => 0,
    };

    update_predicate_status(
        predicate_key,
        PredicateStatus::Scanning(ScanningData {
            number_of_blocks_to_scan,
            number_of_blocks_scanned,
            number_of_times_triggered,
            last_occurrence,
            current_block_height,
        }),
        predicates_db_conn,
        &ctx,
    );
}

fn set_initial_scan_complete_status(
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let current_status = retrieve_predicate_status(&predicate_key, predicates_db_conn);
    let scan_data = match current_status {
        Some(status) => match status {
            PredicateStatus::Scanning(scanning_data) => scanning_data,
            PredicateStatus::Interrupted(_) | PredicateStatus::Disabled => ScanningData {
                ..Default::default()
            },
            PredicateStatus::Streaming(_) | PredicateStatus::InitialScanCompleted(_) => {
                unreachable!("unreachable predicate status: {:?}", status)
            }
        },
        None => ScanningData {
            ..Default::default()
        },
    };
    update_predicate_status(
        predicate_key,
        PredicateStatus::InitialScanCompleted(CompletedScanData {
            number_of_blocks_scanned: scan_data.number_of_blocks_scanned,
            number_of_times_triggered: scan_data.number_of_times_triggered,
            last_occurrence: scan_data.last_occurrence,
            final_block_height_scanned: scan_data.current_block_height,
        }),
        predicates_db_conn,
        &ctx,
    );
}

// todo: consider using this so we can maintain number of times triggered after an interruption.
// what i don't know is - can we continue after an interruption to get further status updates?
pub fn set_predicate_interrupted_status(
    predicate_key: &str,
    error: String,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    update_predicate_status(
        &predicate_key,
        PredicateStatus::Interrupted(error),
        predicates_db_conn,
        &ctx,
    );
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

#[cfg(test)]
pub mod tests;
