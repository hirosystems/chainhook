use crate::config::Config;
use crate::config::EventSourceConfig;
use crate::config::LimitsConfig;
use crate::config::PathConfig;
use crate::config::PredicatesApi;
use crate::config::PredicatesApiConfig;
use crate::config::StorageConfig;
use crate::config::DEFAULT_REDIS_URI;
use crate::service::http_api::start_predicate_api_server;
use crate::service::PredicateStatus;
use crate::service::Service;
use chainhook_sdk::indexer::IndexerConfig;
use chainhook_sdk::observer::ObserverCommand;
use chainhook_sdk::types::BitcoinBlockSignaling;
use chainhook_sdk::types::BitcoinNetwork;
use chainhook_sdk::types::StacksNetwork;
use chainhook_sdk::types::StacksNodeConfig;
use chainhook_sdk::utils::Context;
use redis::Commands;
use rocket::serde::json::Value as JsonValue;
use rocket::Shutdown;
use std::path::PathBuf;
use std::process::Stdio;
use std::process::{Child, Command};
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;

pub async fn get_predicate_status(uuid: &str, port: u16) -> Result<PredicateStatus, String> {
    let mut attempts = 0;
    loop {
        let res = call_get_predicate(uuid, port).await?;
        match res.as_object() {
            Some(res_obj) => match res_obj.get("result") {
                Some(result) => match result.get("status") {
                    Some(status) => {
                        return serde_json::from_value(status.clone())
                            .map_err(|e| format!("failed to parse status {}", e.to_string()));
                    }
                    None => return Err(format!("no status field on get predicate result")),
                },
                None => {
                    attempts += 1;
                    println!("reattempting get predicate status");
                    if attempts == 10 {
                        return Err(format!("no result field on get predicate response"));
                    } else {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            },
            None => return Err(format!("failed to parse get predicate response")),
        }
    }
}

pub async fn filter_predicate_status_from_all_predicates(
    uuid: &str,
    port: u16,
) -> Result<PredicateStatus, String> {
    let mut attempts = 0;
    loop {
        let res = call_get_predicates(port).await?;
        match res.as_object() {
            Some(res_obj) => match res_obj.get("result") {
                Some(result) => match result.as_array() {
                    Some(predicate_array) => {
                        let matching_predicate =
                            predicate_array.iter().find(|p| match p.as_object() {
                                Some(p) => match p.get("uuid") {
                                    Some(predicate_uuid) => predicate_uuid == uuid,
                                    None => false,
                                },
                                None => false,
                            });
                        match matching_predicate {
                            Some(predicate) => match predicate.get("status") {
                                Some(status) => {
                                    return serde_json::from_value(status.clone()).map_err(|e| {
                                        format!("failed to parse status {}", e.to_string())
                                    });
                                }
                                None => {
                                    return Err(format!(
                                        "no status field on matching get predicates result"
                                    ))
                                }
                            },
                            None => {
                                return Err(format!(
                                    "could not find predicate result with uuid matching {uuid}"
                                ));
                            }
                        }
                    }
                    None => {
                        return Err(format!(
                            "failed to parse get predicate response's result field"
                        ))
                    }
                },
                None => {
                    attempts += 1;
                    println!("reattempting get predicates");
                    if attempts == 10 {
                        return Err(format!("no result field on get predicates response"));
                    } else {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            },
            None => return Err(format!("failed to parse get predicate response")),
        }
    }
}

pub async fn call_register_predicate(
    predicate: &JsonValue,
    port: u16,
) -> Result<JsonValue, String> {
    let client = reqwest::Client::new();
    let res =client
            .post(format!("http://localhost:{port}/v1/chainhooks"))
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
                    "Failed to deserialize response of POST request to localhost:{port}/v1/chainhooks: {}",
                    e
                )
            })?;
    Ok(res)
}

pub async fn call_get_predicate(predicate_uuid: &str, port: u16) -> Result<JsonValue, String> {
    let client = reqwest::Client::new();
    let res =client
            .get(format!("http://localhost:{port}/v1/chainhooks/{predicate_uuid}"))
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
                    "Failed to deserialize response of GET request to localhost:{port}/v1/chainhooks: {}",
                    e
                )
            })?;
    Ok(res)
}

pub async fn build_predicate_api_server(port: u16) -> (Receiver<ObserverCommand>, Shutdown) {
    let ctx = Context {
        logger: None,
        tracer: false,
    };
    let api_config = PredicatesApiConfig {
        http_port: port,
        display_logs: true,
        database_uri: DEFAULT_REDIS_URI.to_string(),
    };

    let (tx, rx) = channel();
    let shutdown = start_predicate_api_server(api_config, tx, ctx)
        .await
        .unwrap();

    // Loop to check if the server is ready
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 10;
    loop {
        if attempts >= MAX_ATTEMPTS {
            panic!("failed to start server");
        }

        if let Ok(_client) = reqwest::Client::new()
            .get(format!("http://localhost:{}/ping", port))
            .send()
            .await
        {
            break; // Server is ready
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        attempts += 1;
    }
    (rx, shutdown)
}

pub async fn start_redis(port: u16) -> Result<Child, String> {
    let handle = Command::new("redis-server")
        .arg(format!("--port {port}"))
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to create start-redis command: {}", e.to_string()))?;
    let mut attempts = 0;
    loop {
        match redis::Client::open(format!("redis://localhost:{port}/")) {
            Ok(client) => match client.get_connection() {
                Ok(_) => return Ok(handle),
                Err(e) => {
                    attempts += 1;
                    if attempts == 10 {
                        return Err(format!("failed to start redis service: {}", e.to_string()));
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await
                }
            },
            Err(e) => {
                attempts += 1;
                if attempts == 10 {
                    return Err(format!("failed to start redis service: {}", e.to_string()));
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await
            }
        }
    }
}

pub fn flush_redis(port: u16) {
    let client = redis::Client::open(format!("redis://localhost:{port}/"))
        .expect("unable to connect to redis");
    let mut predicate_db_conn = client.get_connection().expect("unable to connect to redis");
    let predicate_keys: Vec<String> = predicate_db_conn
        .scan_match("predicate:*")
        .unwrap()
        .into_iter()
        .collect();
    for k in predicate_keys {
        predicate_db_conn
            .hdel::<_, _, ()>(&k, "predicates")
            .unwrap();
        predicate_db_conn.hdel::<_, _, ()>(&k, "status").unwrap();
        predicate_db_conn
            .hdel::<_, _, ()>(&k, "specification")
            .unwrap();
    }
}

pub fn get_chainhook_config(
    redis_port: u16,
    chainhook_port: u16,
    stacks_rpc_port: u16,
    stacks_ingestion_port: u16,
    bitcoin_rpc_port: u16,
    working_dir: &str,
    tsv_dir: &str,
) -> Config {
    let api_config = PredicatesApiConfig {
        http_port: chainhook_port,
        display_logs: true,
        database_uri: format!("redis://localhost:{redis_port}/"),
    };
    Config {
        http_api: PredicatesApi::On(api_config),
        storage: StorageConfig {
            working_dir: working_dir.into(),
        },
        event_sources: vec![EventSourceConfig::StacksTsvPath(PathConfig {
            file_path: PathBuf::from(tsv_dir),
        })],
        limits: LimitsConfig {
            max_number_of_bitcoin_predicates: 100,
            max_number_of_concurrent_bitcoin_scans: 100,
            max_number_of_stacks_predicates: 10,
            max_number_of_concurrent_stacks_scans: 10,
            max_number_of_processing_threads: 16,
            max_number_of_networking_threads: 16,
            max_caching_memory_size_mb: 32000,
        },
        network: IndexerConfig {
            bitcoin_network: BitcoinNetwork::Regtest,
            stacks_network: StacksNetwork::Devnet,
            bitcoind_rpc_username: "".into(),
            bitcoind_rpc_password: "".into(),
            bitcoind_rpc_url: format!("http://0.0.0.0:{bitcoin_rpc_port}"),
            bitcoin_block_signaling: BitcoinBlockSignaling::Stacks(StacksNodeConfig {
                rpc_url: format!("http://localhost:{stacks_rpc_port}"),
                ingestion_port: stacks_ingestion_port,
            }),
        },
    }
}
pub async fn start_chainhook_service(
    config: Config,
    chainhook_port: u16,
    ctx: &Context,
) -> Result<(), String> {
    let mut service = Service::new(config, ctx.clone());
    let startup_predicates = vec![];
    let _ = hiro_system_kit::thread_named("Stacks service")
        .spawn(move || {
            let future = service.run(startup_predicates);
            let _ = hiro_system_kit::nestable_block_on(future);
        })
        .map_err(|e| {
            format!(
                "failed to start chainhook service thread, {}",
                e.to_string()
            )
        })?;

    // Loop to check if the server is ready
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 10;
    loop {
        if attempts >= MAX_ATTEMPTS {
            return Err(format!("failed to ping chainhook service"));
        }

        if let Ok(_client) = reqwest::Client::new()
            .get(format!("http://localhost:{}/ping", chainhook_port))
            .send()
            .await
        {
            break Ok(()); // Server is ready
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        attempts += 1;
    }
}
