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
    use std::sync::mpsc::Receiver;

    use chainhook_sdk::chainhooks::types::ChainhookFullSpecification;
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
    // Question
    // Can we generate some payload directly with the open-api spec?
    // It not, that's ok, let's be pragmatic and construct the payloads ourselves
    //
    fn get_bitcoin_if_this_options(
        txid: &str,
        p2pkh_address: &str,
        p2sh_address: &str,
        p2wpkh_address: &str,
        p2wsh_address: &str,
    ) -> Vec<String> {
        vec![
            // Block scope
            format!(
                r#"
            "if_this": {{
                "scope": "block"
            }}"#
            ),
            // Txid scope
            format!(
                r#"
            "if_this": {{
                "scope": "txid",
                "equals": "{txid}"
            }}"#
            ),
            // Inputs scope
            format!(
                r#"
            "if_this": {{
                "scope": "inputs",
                "txid": {{
                    "txid": "{txid}",
                    "vout": 0
                }}
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "inputs",
                "witness_script": {{
                    "equals": "test"
                }}
            }}"#
            ),
            // Outputs scope
            format!(
                r#"
            "if_this": {{
                "scope": "outputs",
                "p2pkh": {{
                    "equals": "{p2pkh_address}"
                }}
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "outputs",
                "p2sh": {{
                    "equals": "{p2sh_address}"
                }}
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "outputs",
                "p2wpkh": {{
                    "equals": "{p2wpkh_address}"
                }}
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "outputs",
                "p2wsh": {{
                    "equals": "{p2wsh_address}"
                }}
            }}"#
            ),
            // StacksProtocol scope
            format!(
                r#"
            "if_this": {{
                "scope": "stacks_protocol",
                "operation": "stacker_rewarded"
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "stacks_protocol",
                "operation": "block_committed"
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "stacks_protocol",
                "operation": "leader_registered"
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "stacks_protocol",
                "operation": "stx_transferred"
            }}"#
            ),
            format!(
                r#"
            "if_this": {{
                "scope": "stacks_protocol",
                "operation": "stx_locked"
            }}"#
            ),
            // OrdinalsProtocol scope
            format!(
                r#"
            "if_this": {{
                "scope": "ordinals_protocol",
                "operation": "inscription_feed"
            }}"#
            ),
        ]
    }

    fn get_bitcoin_then_that_options(http_post_url: &str) -> Vec<String> {
        vec![
            format!(r#""then_that": "noop""#),
            format!(
                r#"
            "then_that": {{
                "http_post": {{
                    "url": "{http_post_url}",
                    "authorization_header": "Bearer FYRPnz2KHj6HueFmaJ8GGD3YMbirEFfh"
                }}
            }}"#
            ),
            format!(
                r#"
            "then_that": {{
                "file_append": {{
                    "path": "./path"
                }}
            }}"#
            ),
        ]
    }
    fn get_combinations(items: Vec<String>) -> Vec<String> {
        // add all combintations of our options,
        // from one option per entry to all options per entry
        let mut combinations = vec![];
        for (i, _) in items.iter().enumerate() {
            combinations.append(
                &mut items
                    .iter()
                    .combinations(i)
                    .map(|e| e.iter().join(","))
                    .collect_vec(),
            )
        }
        // the above doesn't include an entry of all options or an entry of no options
        combinations.push(items.clone().join(","));
        combinations.push(String::new());
        combinations
    }
    fn get_bitcoin_filter_combinations() -> Vec<String> {
        let options = vec![
            format!(
                r#"
            "start_block": 0"#
            ),
            format!(
                r#"
            "end_block": 0"#
            ),
            format!(
                r#"
            "expire_after_occurrence": 0"#
            ),
            format!(
                r#"
            "capture_all_events": true"#
            ),
            format!(
                r#"
            "decode_clarity_values": true"#
            ),
        ];
        get_combinations(options)
    }

    fn build_bitcoin_payloads(
        uuid: &str,
        txid: &str,
        p2pkh_address: &str,
        p2sh_address: &str,
        p2wpkh_address: &str,
        p2wsh_address: &str,
        http_post_url: &str,
    ) -> Vec<String> {
        let mut payloads = vec![];
        let networks = vec!["mainnet", "testnet", "regtest"];
        for network in networks {
            for if_this in get_bitcoin_if_this_options(
                txid,
                p2pkh_address,
                p2sh_address,
                p2wpkh_address,
                p2wsh_address,
            ) {
                for then_that in get_bitcoin_then_that_options(http_post_url) {
                    for mut filter in get_bitcoin_filter_combinations() {
                        let filter_str = {
                            if filter == String::new() {
                                filter
                            } else {
                                filter.push_str(",");
                                filter
                            }
                        };
                        payloads.push(format!(
                            r#"
{{
    "chain": "bitcoin",
    "uuid": "{uuid}",
    "name": "test",
    "version": 1,
    "networks": {{
        "{network}": {{{filter_str}{if_this},
            {then_that}
        }}
    }}
}}"#
                        ))
                    }
                }
            }
        }

        payloads
    }
    // pub fn build_bitcoin_p2sh_predicate_payload(p2pkh_address: String) {

    // }

    // pub fn build_bitcoin_p2pkh_predicate_payload(p2pkh_address: String) {

    // }

    // pub fn build_bitcoin_p2pkh_predicate_payload(p2pkh_address: String) {

    // }

    // pub fn build_bitcoin_p2pkh_predicate_payload(p2pkh_address: String) {

    // }
    pub fn build_ctx() -> Context {
        let logger = hiro_system_kit::log::setup_logger();
        let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
        let ctx = Context {
            logger: None,
            tracer: false,
        };
        ctx
    }

    async fn build_service() -> Receiver<ObserverCommand> {
        // Build config
        let ctx = build_ctx();
        let api_config = PredicatesApiConfig {
            http_port: 8765,
            display_logs: true,
            database_uri: DEFAULT_REDIS_URI.to_string(),
        };
        // Build service
        let (tx, rx) = channel();
        println!("starting predicate api server");
        start_predicate_api_server(api_config, tx, ctx)
            .await
            .unwrap();
        println!("started predicate api server");
        rx
        // Build and spinup event observer

        // Build a timeline of events / expectations
    }

    async fn call_register_predicate(predicate: String) -> JsonValue {
        let client = reqwest::Client::new();
        client
            .post("http://localhost:8765/v1/chainhooks")
            .header("Content-Type", "application/json")
            .body(predicate)
            .send()
            .await
            .expect("Failed to make POST request to localhost:8765/v1/chainhooks")
            .json::<JsonValue>()
            .await
            .expect(
                "Failed to deserialize response of POST request to localhost:8765/v1/chainhooks",
            )
    }

    #[tokio::test]
    async fn it_registers_a_predicate() {
        println!("runnign test");
        let uuid = "4ecc-4ecc-435b-9948-d5eeca1c3ce6";
        let txid = "";
        let p2pkh_address = "";
        let p2sh_address = "";
        let p2wpkh_address = "";
        let p2wsh_address = "";
        let http_post_url = "http://localhost:1234";
        let payloads = build_bitcoin_payloads(
            uuid,
            txid,
            p2pkh_address,
            p2sh_address,
            p2wpkh_address,
            p2wsh_address,
            http_post_url,
        );

        let rx = build_service().await;

        for predicate in payloads {
            let res = call_register_predicate(predicate.clone()).await;
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
                    let deserialized_predicate: ChainhookFullSpecification =
                        serde_json::from_str(&predicate).unwrap();
                    assert_eq!(registered_predicate, deserialized_predicate);
                }
                _ => panic!("Received wrong observer command for predicate registration"),
            }
        }
    }
}
