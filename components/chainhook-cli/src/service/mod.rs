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

    const UUID: &str = "4ecc-4ecc-435b-9948-d5eeca1c3ce6";

    fn build_bitcoin_payload(
        network: Option<&str>,
        if_this: Option<JsonValue>,
        then_that: Option<JsonValue>,
        filter: Option<JsonValue>,
    ) -> JsonValue {
        let network = network.unwrap_or("mainnet");
        let if_this = if_this.unwrap_or(json!({"scope":"block"}));
        let then_that = then_that.unwrap_or(json!("noop"));
        let filter = filter.unwrap_or(json!({}));

        let filter = filter.as_object().unwrap();
        let mut network_val = json!({
            "if_this": if_this,
            "then_that": then_that
        });
        for (k, v) in filter.iter() {
            network_val[k] = v.to_owned();
        }
        json!({
            "chain": "bitcoin",
            "uuid": UUID,
            "name": "test",
            "version": 1,
            "networks": {
                network: network_val
            }
        })
    }

    async fn build_service() -> (Receiver<ObserverCommand>, Shutdown) {
        let ctx = Context {
            logger: None,
            tracer: false,
        };
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

    async fn call_register_predicate(predicate: &JsonValue) -> Result<JsonValue, String> {
        let client = reqwest::Client::new();
        let res =client
            .post(format!("http://localhost:8675/v1/chainhooks"))
            .header("Content-Type", "application/json")
            .json(predicate)
            .send()
            .await
            .map_err(|e| {
                format!(
                    "Failed to make POST request to localhost:8765/v1/chainhooks: {}",
                    e
                )
            })?
            .json::<JsonValue>()
            .await
            .map_err(|e| {
                format!(
                    "Failed to deserialize response of POST request to localhost:8765/v1/chainhooks: {}",
                    e
                )
            })?;
        Ok(res)
    }

    async fn test_register_predicate(predicate: JsonValue) -> Result<(), (String, Shutdown)> {
        let (rx, shutdown) = build_service().await;

        let moved_shutdown = shutdown.clone();
        let res = call_register_predicate(&predicate)
            .await
            .map_err(|e| (e, moved_shutdown))?;

        let moved_shutdown = shutdown.clone();
        let (status, result) = match res {
            JsonValue::Object(obj) => {
                if let Some(err) = obj.get("error") {
                    shutdown.notify();
                    panic!("Register predicate result contained error: {}", err);
                }
                let status = obj.get("status").unwrap().to_string();
                let result = obj.get("result").unwrap().to_string();
                Ok((status, result))
            }
            _ => Err(format!("Register predicate result is not correct type")),
        }
        .map_err(|e| (e, moved_shutdown))?;

        let moved_shutdown = shutdown.clone();
        let command = rx.recv().map_err(|e| {
            (
                format!("Channel error for predicate registration: {}", e),
                moved_shutdown,
            )
        })?;

        let moved_shutdown = shutdown.clone();
        let registered_predicate = match command {
            ObserverCommand::RegisterPredicate(registered_predicate) => {
                let registered_predicate: JsonValue =
                    serde_json::from_str(&serde_json::to_string(&registered_predicate).unwrap())
                        .unwrap();
                Ok(registered_predicate)
            }
            _ => Err(format!(
                "Received wrong observer command for predicate registration"
            )),
        }
        .map_err(|e| (e, moved_shutdown))?;

        shutdown.notify();
        assert_eq!(registered_predicate, predicate);
        assert_eq!(status, String::from("200"));
        assert_eq!(result, format!("\"{UUID}\""));
        Ok(())
    }

    #[test_case("mainnet" ; "mainnet")]
    #[test_case("testnet" ; "testnet")]
    #[test_case("regtest" ; "regtest")]
    #[serial_test::serial]
    #[tokio::test]
    async fn it_handles_bitcoin_predicates_with_network(network: &str) {
        let predicate = build_bitcoin_payload(Some(network), None, None, None);
        match test_register_predicate(predicate).await {
            Ok(_) => {}
            Err((e, shutdown)) => {
                shutdown.notify();
                panic!("{e}");
            }
        }
    }

    #[test_case(json!({"scope":"block"}); "with scope block")]
    #[test_case(json!({"scope":"txid", "equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"}) ; "with scope txid")]
    #[test_case(json!({"scope": "inputs","txid": {"txid": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f","vout": 0}}) ; "with scope inputs type txid")]
    #[test_case(json!({"scope": "inputs","witness_script": {"equals": "test"}}) ; "with scope inputs type witness_script equal match")]
    #[test_case(json!({"scope": "inputs","witness_script": {"starts_with": "test"}}) ; "with scope inputs type witness_script starts_with match")]
    #[test_case(json!({"scope": "inputs","witness_script": {"ends_with": "test"}}) ; "with scope inputs type witness_script ends_with match")]
    #[test_case(json!({"scope": "outputs","op_return": {"equals": "0x69bd04208265aca9424d0337dac7d9e84371a2c91ece1891d67d3554bd9fdbe60afc6924d4b0773d90000006700010000006600012"}}) ; "with scope outputs type op_return equal match")]
    #[test_case(json!({"scope": "outputs","op_return": {"starts_with": "X2["}}) ; "with scope outputs type op_return starts_with match")]
    #[test_case(json!({"scope": "outputs","op_return": {"ends_with": "0x76a914000000000000000000000000000000000000000088ac"}}) ; "with scope outputs type op_return ends_with match")]
    #[test_case(json!({"scope": "outputs","p2pkh": {"equals": "mr1iPkD9N3RJZZxXRk7xF9d36gffa6exNC"}}) ; "with scope outputs type p2pkh")]
    #[test_case(json!({ "scope": "outputs","p2sh": {"equals": "2MxDJ723HBJtEMa2a9vcsns4qztxBuC8Zb2"}}) ; "with scope outputs type p2sh")]
    #[test_case(json!({"scope": "outputs","p2wpkh": {"equals": "bcrt1qnxknq3wqtphv7sfwy07m7e4sr6ut9yt6ed99jg"}}) ; "with scope outputs type p2wpkh")]
    #[test_case(json!({"scope": "outputs","p2wsh": {"equals": "bc1qklpmx03a8qkv263gy8te36w0z9yafxplc5kwzc"}}) ; "with scope outputs type p2wsh")]
    #[test_case(json!({"scope": "stacks_protocol","operation": "stacker_rewarded"}) ; "with scope stacks_protocol operation stacker_rewarded")]
    #[test_case(json!({"scope": "stacks_protocol","operation": "block_committed"}) ; "with scope stacks_protocol operation block_committed")]
    #[test_case(json!({"scope": "stacks_protocol","operation": "leader_registered"}) ; "with scope stacks_protocol operation leader_registered")]
    #[test_case(json!({"scope": "stacks_protocol","operation": "stx_transferred"}) ; "with scope stacks_protocol operation stx_transferred")]
    #[test_case(json!({"scope": "stacks_protocol","operation": "stx_locked"}) ; "with scope stacks_protocol operation stx_locked")]
    #[test_case(json!({"scope": "ordinals_protocol","operation": "inscription_feed"}) ; "with scope ordinals_protocol operation inscription_feed")]
    #[serial_test::serial]
    #[tokio::test]
    async fn it_handles_bitcoin_if_this_predicates(if_this: JsonValue) {
        let predicate = build_bitcoin_payload(None, Some(if_this), None, None);
        match test_register_predicate(predicate).await {
            Ok(_) => {}
            Err((e, shutdown)) => {
                shutdown.notify();
                panic!("{e}");
            }
        }
    }

    #[test_case(json!("noop") ; "with noop action")]
    #[test_case(json!({"http_post": {"url": "http://localhost:1234", "authorization_header": "Bearer FYRPnz2KHj6HueFmaJ8GGD3YMbirEFfh"}}) ; "with http_post action")]
    #[test_case(json!({"file_append": {"path": "./path"}}) ; "with file_append action")]
    #[serial_test::serial]
    #[tokio::test]
    async fn it_handles_bitcoin_then_that_predicates(then_that: JsonValue) {
        let predicate = build_bitcoin_payload(None, None, Some(then_that), None);
        match test_register_predicate(predicate).await {
            Ok(_) => {}
            Err((e, shutdown)) => {
                shutdown.notify();
                panic!("{e}");
            }
        }
    }

    #[test_case(json!({"start_block": 0,"end_block": 0,"expire_after_occurrence": 0,"include_proof": true,"include_inputs": true,"include_outputs": true,"include_witness": true}) ; "all filters")]
    #[test_case(json!({"start_block": 0}) ; "start_block filter")]
    #[test_case(json!({"end_block": 0}) ; "end_block filter")]
    #[test_case(json!({"expire_after_occurrence": 0}) ; "expire_after_occurrence filter")]
    #[test_case(json!({"include_proof": true}) ; "include_proof filter")]
    #[test_case(json!({"include_inputs": true}) ; "include_inputs filter")]
    #[test_case(json!({"include_outputs": true}) ; "include_outputs filter")]
    #[test_case(json!({"include_witness": true}) ; "include_witness filter")]
    #[serial_test::serial]
    #[tokio::test]
    async fn it_handles_bitcoin_predicates_with_filters(filters: JsonValue) {
        let predicate = build_bitcoin_payload(None, None, None, Some(filters));
        match test_register_predicate(predicate).await {
            Ok(_) => {}
            Err((e, shutdown)) => {
                shutdown.notify();
                panic!("{e}");
            }
        }
    }
}
