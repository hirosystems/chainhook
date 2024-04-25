use std::{sync::mpsc::channel, thread::sleep, time::Duration};

use chainhook_sdk::{
    observer::{config::EventObserverConfig, start_event_observer},
    types::{BitcoinNetwork, StacksNodeConfig},
    utils::Context,
};
use reqwest::Method;
use serde_json::Value;
use test_case::test_case;

use crate::service::tests::{
    helpers::{
        build_predicates::build_stacks_payload,
        mock_service::{
            call_observer_svc, call_ping, call_prometheus, call_register_predicate, flush_redis,
            TestSetupResult,
        },
    },
    setup_bitcoin_chainhook_test, setup_stacks_chainhook_test,
};

use super::helpers::{build_predicates::get_random_uuid, get_free_port};

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn ping_endpoint_returns_metrics() {
    let TestSetupResult {
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        stacks_rpc_port: _,
        bitcoin_rpc_port: _,
        prometheus_port: _,
        observer_command_tx: _,
    } = setup_stacks_chainhook_test(1, None, None).await;

    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(Some("devnet"), None, None, None, Some(uuid));
    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    let metrics = call_ping(stacks_ingestion_port).await.unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });
    let result = metrics
        .get("stacks")
        .unwrap()
        .get("registered_predicates")
        .unwrap();
    assert_eq!(result, 1);

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn prometheus_endpoint_returns_encoded_metrics() {
    let TestSetupResult {
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port: _,
        stacks_rpc_port: _,
        bitcoin_rpc_port: _,
        prometheus_port,
        observer_command_tx: _,
    } = setup_stacks_chainhook_test(1, None, None).await;

    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(Some("devnet"), None, None, None, Some(uuid));
    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    let metrics = call_prometheus(prometheus_port).await.unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });
    const EXPECTED: &'static str = "# HELP chainhook_stx_registered_predicates The number of Stacks predicates that have been registered by the Chainhook node.\n# TYPE chainhook_stx_registered_predicates gauge\nchainhook_stx_registered_predicates 1\n";
    assert!(metrics.contains(EXPECTED));

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}

async fn await_observer_started(port: u16) {
    let mut attempts = 0;
    loop {
        let url = format!("http://localhost:{port}/ping");
        match call_observer_svc(&url, Method::GET, None).await {
            Ok(_) => break,
            Err(e) => {
                if attempts > 3 {
                    panic!("failed to start event observer, {}", e);
                } else {
                    attempts += 1;
                    sleep(Duration::new(0, 500_000_000));
                }
            }
        }
    }
}
#[test_case("/wallet", json!({
    "method": "getaddressinfo",
    "params": vec!["bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"],
    "id": "my-id",
    "jsonrpc": "2.0"
}))]
#[test_case("/", json!({
    "method": "sendrawtransaction",
    "params": vec!["0x0000"],
    "id": "my-id",
    "jsonrpc": "2.0"
}))]
#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn bitcoin_rpc_requests_are_forwarded(endpoint: &str, body: Value) {
    let TestSetupResult {
        mut redis_process,
        working_dir,
        chainhook_service_port: _,
        redis_port,
        stacks_ingestion_port,
        stacks_rpc_port: _,
        bitcoin_rpc_port: _,
        prometheus_port: _,
        observer_command_tx: _,
    } = setup_bitcoin_chainhook_test(1).await;

    await_observer_started(stacks_ingestion_port).await;

    let url = format!("http://localhost:{stacks_ingestion_port}{endpoint}");
    let response = call_observer_svc(&url, Method::POST, Some(&body))
        .await
        .unwrap();
    assert!(response.get("result").is_some());
    assert!(response.get("error").is_none());
    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}

async fn start_and_ping_event_observer(config: EventObserverConfig, ingestion_port: u16) {
    let (observer_commands_tx, observer_commands_rx) = channel();
    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };
    start_event_observer(
        config,
        observer_commands_tx,
        observer_commands_rx,
        None,
        None,
        None,
        ctx,
    )
    .unwrap();
    await_observer_started(ingestion_port).await;
}
#[test_case("/drop_mempool_tx", Method::POST, None)]
#[test_case("/attachments/new", Method::POST, None)]
#[test_case("/mined_block", Method::POST, Some(&json!({})))]
#[test_case("/mined_microblock", Method::POST, Some(&json!({})))]
#[tokio::test]
async fn it_responds_200_for_unimplemented_endpoints(
    endpoint: &str,
    method: Method,
    body: Option<&Value>,
) {
    let ingestion_port = get_free_port().unwrap();
    let config = EventObserverConfig {
        chainhook_config: None,
        bitcoin_rpc_proxy_enabled: false,
        bitcoind_rpc_username: format!(""),
        bitcoind_rpc_password: format!(""),
        bitcoind_rpc_url: format!(""),
        bitcoin_block_signaling: chainhook_sdk::types::BitcoinBlockSignaling::Stacks(
            StacksNodeConfig {
                rpc_url: format!(""),
                ingestion_port: ingestion_port,
            },
        ),
        display_stacks_ingestion_logs: false,
        bitcoin_network: BitcoinNetwork::Regtest,
        stacks_network: chainhook_sdk::types::StacksNetwork::Devnet,
        prometheus_monitoring_port: None,
    };
    start_and_ping_event_observer(config, ingestion_port).await;
    let url = format!("http://localhost:{ingestion_port}{endpoint}");
    let response = call_observer_svc(&url, method, body).await.unwrap();
    assert_eq!(response.get("status").unwrap(), &json!(200));
}
