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

        let _ = start_event_observer(
            event_observer_config.clone(),
            observer_command_tx,
            observer_command_rx,
            Some(observer_event_tx),
            self.ctx.clone(),
        );

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

                    for (_predicate_uuid, _blocks_ids) in report.predicates_evaluated.iter() {}

                    for (_predicate_uuid, _blocks_ids) in report.predicates_triggered.iter() {}
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
    InitialScanCompleted,
    Interrupted(String),
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanningData {
    pub number_of_blocks_to_scan: u64,
    pub number_of_blocks_scanned: u64,
    pub number_of_blocks_sent: u64,
    pub current_block_height: u64,
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

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use rocket::serde::json::Value as JsonValue;
    use rocket::Shutdown;
    use std::sync::mpsc::Receiver;
    use test_case::test_case;

    use chainhook_sdk::observer::ObserverCommand;

    use crate::config::PredicatesApiConfig;
    use crate::config::DEFAULT_REDIS_URI;

    use super::channel;
    use super::http_api::start_predicate_api_server;
    use super::Context;

    #[derive(Serialize, Deserialize)]
    struct RegisterPredicateResponse {
        status: u64,
        result: String,
    }

    fn _get_bitcoin_if_this_options() -> Vec<JsonValue> {
        vec![
            // Block scope
            json!({"scope": "block"}),
            // Txid scope
            json!({
                "scope": "txid",
                "equals": "{txid}"
            }),
            // Inputs scope
            json!({
                "scope": "inputs",
                "txid": {
                    "txid": "{txid}",
                    "vout": 0
                }
            }),
            json!({
                "scope": "inputs",
                "witness_script": {
                    "equals": "test"
                }
            }),
            // Outputs scope
            json!({
                "scope": "outputs",
                "p2pkh": {
                    "equals": "{p2pkh_address}"
                }
            }),
            json!({
                "scope": "outputs",
                "p2sh": {
                    "equals": "{p2sh_address}"
                }
            }),
            json!({
                "scope": "outputs",
                "p2wpkh": {
                    "equals": "{p2wpkh_address}"
                }
            }),
            json!({
                "scope": "outputs",
                "p2wsh": {
                    "equals": "{p2wsh_address}"
                }
            }),
            // StacksProtocol scope
            json!({
                "scope": "stacks_protocol",
                "operation": "stacker_rewarded"
            }),
            json!({
                "scope": "stacks_protocol",
                "operation": "block_committed"
            }
            ),
            json!({
                "scope": "stacks_protocol",
                "operation": "leader_registered"
            }),
            json!({
                "scope": "stacks_protocol",
                "operation": "stx_transferred"
            }),
            json!({
                "scope": "stacks_protocol",
                "operation": "stx_locked"
            }),
            // OrdinalsProtocol scope
            json!({
                "scope": "ordinals_protocol",
                "operation": "inscription_feed"
            }),
        ]
    }

    fn get_bitcoin_then_that_options() -> Vec<JsonValue> {
        vec![
            json!("noop"),
            json!({
                "http_post": {
                    "url": "http://localhost:1234",
                    "authorization_header": "Bearer FYRPnz2KHj6HueFmaJ8GGD3YMbirEFfh"
                }
            }),
            json!({
                "file_append": {
                    "path": "./path"
                }
            }),
        ]
    }

    fn get_combintations(items: JsonValue) -> Vec<JsonValue> {
        let obj = items.as_object().unwrap();
        let mut all_combinations = vec![];
        let keys = obj.keys();
        for (i, _) in keys.enumerate() {
            let combinations = obj.into_iter().combinations(i);
            for entries in combinations {
                let mut joined_entry = json!({});
                for (k, v) in entries {
                    joined_entry[k] = v.to_owned();
                }
                all_combinations.push(joined_entry);
            }
        }
        all_combinations
    }

    fn get_bitcoin_filter_combinations() -> Vec<JsonValue> {
        let things = json!({
            "start_block": 0,
            "end_block": 0,
            "expire_after_occurrence": 0,
            "include_proof": true,
            "include_inputs": true,
            "include_outputs": true,
            "include_witness": true,
        });
        get_combintations(things)
    }

    fn build_bitcoin_payloads(network: &str, if_this: JsonValue, uuid: &str) -> Vec<JsonValue> {
        let mut payloads = vec![];

        for then_that in get_bitcoin_then_that_options() {
            for filter in get_bitcoin_filter_combinations() {
                let filter = filter.as_object().unwrap();
                let mut network_val = json!({
                    "if_this": if_this,
                    "then_that": then_that
                });
                for (k, v) in filter.iter() {
                    network_val[k] = v.to_owned();
                }
                payloads.push(json!({
                    "chain": "bitcoin",
                    "uuid": uuid,
                    "name": "test",
                    "version": 1,
                    "networks": {
                        network: network_val
                    }
                }));
                payloads.push(json!({
                    "chain": "bitcoin",
                    "uuid": uuid,
                    "owner_uuid": "owner-uuid",
                    "name": "test",
                    "version": 1,
                    "networks": {
                        network: network_val
                    }
                }));
            }
        }

        payloads
    }

    pub fn build_ctx() -> Context {
        let logger = hiro_system_kit::log::setup_logger();
        let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
        let ctx = Context {
            logger: None,
            tracer: false,
        };
        ctx
    }

    async fn build_service() -> (Receiver<ObserverCommand>, Shutdown) {
        let ctx = build_ctx();
        let api_config = PredicatesApiConfig {
            http_port: 8675,
            display_logs: true,
            database_uri: DEFAULT_REDIS_URI.to_string(),
        };

        let (tx, rx) = channel();
        let shutdown = start_predicate_api_server(api_config, tx, ctx)
            .await
            .unwrap();
        (rx, shutdown)
    }

    async fn call_register_predicate(predicate: &JsonValue) -> JsonValue {
        let client = reqwest::Client::new();
        client
            .post(format!("http://localhost:8675/v1/chainhooks"))
            .header("Content-Type", "application/json")
            .json(predicate)
            .send()
            .await
            .expect("Failed to make POST request to localhost:8765/v1/chainhooks")
            .json::<JsonValue>()
            .await
            .expect(
                "Failed to deserialize response of POST request to localhost:8765/v1/chainhooks",
            )
    }

    #[test_case("mainnet", json!({"scope":"block"}); "for mainnet if_this block predicates")]
    #[test_case("mainnet", json!({"scope":"txid", "equals": ""}) ; "for mainnet if_this txid predicates")]
    #[test_case("mainnet", json!({"scope": "inputs","txid": {"txid": "","vout": 0}}) ; "for mainnet if_this txid input predicates")]
    #[test_case("mainnet", json!({"scope": "inputs","witness_script": {"equals": "test"}}) ; "for mainnet if_this witness_script input predicates")]
    #[test_case("mainnet", json!({"scope": "outputs","p2pkh": {"equals": "test"}}) ; "for mainnet if_this p2pkh output predicates")]
    #[test_case("mainnet", json!({ "scope": "outputs","p2sh": {"equals": "test"}}) ; "for mainnet if_this p2sh output predicates")]
    #[test_case("mainnet", json!({"scope": "outputs","p2wpkh": {"equals": "test"}}) ; "for mainnet if_this p2wpkh output predicates")]
    #[test_case("mainnet", json!({"scope": "outputs","p2wsh": {"equals": "test"}}) ; "for mainnet if_this p2wsh output predicates")]
    #[serial_test::serial]
    #[tokio::test]
    async fn it_registers_all_bitcoin_predicates(network: &str, if_this: JsonValue) {
        let uuid = "4ecc-4ecc-435b-9948-d5eeca1c3ce6";
        let payloads = build_bitcoin_payloads(network, if_this, uuid);
        let (rx, shutdown) = build_service().await;
        println!("checking {} predicates", payloads.len());
        let mut i = 0;
        for predicate in payloads {
            let predicate_moved = predicate.clone();
            let res = call_register_predicate(&predicate.clone()).await;

            let (status, result) = match res {
                JsonValue::Object(obj) => {
                    if let Some(err) = obj.get("error") {
                        panic!("Register predicate result contained error: {}", err);
                    }
                    let status = obj.get("status").unwrap().to_string();
                    let result = obj.get("result").unwrap().to_string();
                    (status, result)
                }
                _ => panic!("Register predicate result is not correct type"),
            };
            assert_eq!(status, String::from("200"));
            assert_eq!(result, format!("\"{uuid}\""));

            let command = match rx.recv() {
                Ok(cmd) => cmd,
                Err(e) => panic!("Channel error for predicate registration: {}", e),
            };

            match command {
                ObserverCommand::RegisterPredicate(registered_predicate) => {
                    let registered_predicate: JsonValue = serde_json::from_str(
                        &serde_json::to_string(&registered_predicate).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(registered_predicate, predicate_moved);
                    i = i + 1;
                }
                _ => panic!("Received wrong observer command for predicate registration"),
            }
        }

        println!("checked {i} predicates");
        shutdown.notify();
    }
}
