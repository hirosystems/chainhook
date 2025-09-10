use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use chainhook_sdk::chainhooks::database::PredicatesDatabaseAccess;
use chainhook_sdk::chainhooks::stacks::StacksChainhookInstance;
use chainhook_sdk::chainhooks::types::{ExpiredData, PredicateStatus, ScanningData, StreamingData};
use chainhook_sdk::chainhooks::{bitcoin::BitcoinChainhookInstance, types::ChainhookInstance};
use chainhook_sdk::observer::PredicateEvaluationReport;
use chainhook_sdk::types::Chain;
use chainhook_sdk::utils::Context;
use redis::{Commands, Connection};

use crate::config::PredicatesApiConfig;

#[derive(Clone)]
pub struct RedisPredicatesDatabaseAccess {
    redis_uri: String,
}

impl RedisPredicatesDatabaseAccess {
    pub fn new(redis_uri: String) -> Self {
        Self { redis_uri }
    }

    fn get_predicate_db_conn(&self) -> Result<Connection, String> {
        let client = redis::Client::open(self.redis_uri.as_str())
            .map_err(|e| format!("unable to connect to redis: {}", e))?;
        let conn = client
            .get_connection()
            .map_err(|e| format!("unable to connect to redis: {}", e))?;
        Ok(conn)
    }
}

impl PredicatesDatabaseAccess for RedisPredicatesDatabaseAccess {
    fn get_predicate(&self, uuid: &String, ctx: &Context) -> Result<Option<(ChainhookInstance, PredicateStatus)>, String> {
        let mut conn = self.get_predicate_db_conn()?;
        let predicate_key = ChainhookInstance::either_stx_or_btc_key(uuid);
        get_entry_from_predicates_db(&predicate_key, &mut conn, ctx)
    }

    fn insert_predicate(&self, predicate: ChainhookInstance, ctx: &Context) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;

        update_predicate_spec(&predicate.key(), &predicate, &mut conn, ctx);
        update_predicate_status(&predicate.key(), PredicateStatus::New, &mut conn, ctx);

        Ok(())
    }

    fn enable_predicate(&self, predicate: ChainhookInstance, ctx: &Context) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;

        update_predicate_spec(&predicate.key(), &predicate, &mut conn, ctx);
        set_predicate_streaming_status(
            StreamingDataType::FinishedScanning,
            &predicate.key(),
            &mut conn,
            ctx,
        );

        Ok(())
    }

    fn delete_predicate(&self, uuid: &String, _ctx: &Context) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;
        let predicate_key = ChainhookInstance::either_stx_or_btc_key(uuid);
        conn.del::<_, ()>(predicate_key.clone())
            .map_err(|e| format!("unable to delete predicate: {e}"))?;
        Ok(())
    }

    fn expire_stacks_predicates_for_block(
        &self,
        block_height: u64,
        ctx: &Context,
    ) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;
        expire_predicates_for_block(&Chain::Stacks, block_height, &mut conn, ctx);
        Ok(())
    }

    fn expire_bitcoin_predicates_for_block(
        &self,
        block_height: u64,
        ctx: &Context,
    ) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;
        expire_predicates_for_block(&Chain::Bitcoin, block_height, &mut conn, ctx);
        Ok(())
    }

    fn interrupt_predicate(
        &self,
        uuid: &String,
        error: String,
        ctx: &Context,
    ) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;
        let predicate_key = ChainhookInstance::either_stx_or_btc_key(uuid);
        set_predicate_interrupted_status(error, &predicate_key, &mut conn, ctx);
        Ok(())
    }

    fn update_stacks_predicates_from_report(&self, report: PredicateEvaluationReport, ctx: &Context) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;
        update_status_from_report(Chain::Stacks, report, &mut conn, ctx);
        Ok(())
    }

    fn update_bitcoin_predicates_from_report(&self, report: PredicateEvaluationReport, ctx: &Context) -> Result<(), String> {
        let mut conn = self.get_predicate_db_conn()?;
        update_status_from_report(Chain::Bitcoin, report, &mut conn, ctx);
        Ok(())
    }

    fn get_all_predicates(&self, ctx: &Context) -> Result<Vec<(ChainhookInstance, PredicateStatus)>, String> {
        let mut conn = self.get_predicate_db_conn()?;
        let entries = get_entries_from_predicates_db(&mut conn, ctx)?;
        Ok(entries)
    }

    fn get_active_stacks_predicates(
        &self,
        ctx: &Context,
    ) -> Result<Vec<(StacksChainhookInstance, PredicateStatus)>, String> {
        let mut conn = self.get_predicate_db_conn()?;
        let entries = get_entries_from_predicates_db(&mut conn, ctx)?;
        let stacks_predicates = entries
            .into_iter()
            .filter_map(|(spec, status)| match spec {
                ChainhookInstance::Stacks(stacks_spec) => {
                    if stacks_spec.enabled
                        && !matches!(
                            status,
                            PredicateStatus::ConfirmedExpiration(_)
                                | PredicateStatus::UnconfirmedExpiration(_)
                                | PredicateStatus::Interrupted(_)
                        )
                    {
                        Some((stacks_spec, status))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        Ok(stacks_predicates)
    }

    fn get_active_bitcoin_predicates(
        &self,
        ctx: &Context,
    ) -> Result<Vec<(BitcoinChainhookInstance, PredicateStatus)>, String> {
        let mut conn = self.get_predicate_db_conn()?;
        let entries = get_entries_from_predicates_db(&mut conn, ctx)?;
        let bitcoin_predicates = entries
            .into_iter()
            .filter_map(|(spec, status)| match spec {
                ChainhookInstance::Bitcoin(bitcoin_spec) => {
                    if bitcoin_spec.enabled
                        && !matches!(
                            status,
                            PredicateStatus::ConfirmedExpiration(_)
                                | PredicateStatus::UnconfirmedExpiration(_)
                                | PredicateStatus::Interrupted(_)
                        )
                    {
                        Some((bitcoin_spec, status))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        Ok(bitcoin_predicates)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum StreamingDataType {
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

pub fn update_predicate_spec(
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

pub fn set_predicate_interrupted_status(
    error: String,
    predicate_key: &str,
    predicates_db_conn: &mut Connection,
    ctx: &Context,
) {
    let status = PredicateStatus::Interrupted(error);
    update_predicate_status(predicate_key, status, predicates_db_conn, ctx);
}

fn get_entry_from_predicates_db(
    predicate_key: &str,
    predicate_db_conn: &mut Connection,
    _ctx: &Context,
) -> Result<Option<(ChainhookInstance, PredicateStatus)>, String> {
    let entry: HashMap<String, String> = predicate_db_conn.hgetall(predicate_key).map_err(|e| {
        format!(
            "unable to load chainhook associated with key {}: {}",
            predicate_key, e
        )
    })?;

    let encoded_spec = match entry.get("specification") {
        None => return Ok(None),
        Some(payload) => payload,
    };

    let spec = ChainhookInstance::deserialize_specification(encoded_spec)?;

    let encoded_status = match entry.get("status") {
        None => Err(format!(
            "found predicate specification with no status for predicate {}",
            predicate_key
        )),
        Some(payload) => Ok(payload),
    }?;

    let status = serde_json::from_str(encoded_status).map_err(|e| format!("{}", e))?;

    Ok(Some((spec, status)))
}

fn get_entries_from_predicates_db(
    predicate_db_conn: &mut Connection,
    ctx: &Context,
) -> Result<Vec<(ChainhookInstance, PredicateStatus)>, String> {
    let chainhooks_to_load: Vec<String> = predicate_db_conn
        .scan_match(ChainhookInstance::either_stx_or_btc_key("*"))
        .map_err(|e| format!("unable to connect to redis: {}", e))?
        .collect();

    let mut predicates = vec![];
    for predicate_key in chainhooks_to_load.iter() {
        let chainhook = match get_entry_from_predicates_db(predicate_key, predicate_db_conn, ctx) {
            Ok(Some((spec, status))) => (spec, status),
            Ok(None) => {
                warn!(
                    ctx.expect_logger(),
                    "unable to load chainhook associated with key {}", predicate_key,
                );
                continue;
            }
            Err(e) => {
                error!(
                    ctx.expect_logger(),
                    "unable to load chainhook associated with key {}: {}",
                    predicate_key,
                    e.to_string()
                );
                continue;
            }
        };
        predicates.push(chainhook);
    }
    Ok(predicates)
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
