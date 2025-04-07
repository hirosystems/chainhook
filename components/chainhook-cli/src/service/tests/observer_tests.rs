use std::{sync::mpsc::channel, thread::sleep, time::Duration};

use chainhook_sdk::{
    chainhooks::types::ChainhookStore,
    observer::{start_event_observer, EventObserverConfig, PredicatesConfig},
    types::{BitcoinNetwork, StacksNodeConfig},
    utils::Context,
};
use reqwest::Method;
use serde_json::Value;
use test_case::test_case;

use crate::service::tests::{
    cleanup, cleanup_err,
    helpers::{
        build_predicates::build_stacks_payload,
        mock_service::{
            call_observer_svc, call_ping, call_prometheus, call_register_predicate, flush_redis,
            TestSetupResult,
        },
    },
    setup_bitcoin_chainhook_test, setup_stacks_chainhook_test,
};

use super::helpers::{
    build_predicates::get_random_uuid, get_free_port, mock_stacks_node::create_tmp_working_dir,
};

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn ping_endpoint_returns_metrics() -> Result<(), String> {
    let TestSetupResult {
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
        .map_err(|e| cleanup_err(e, &working_dir, redis_port))?;

    sleep(Duration::new(1, 0));
    let metrics = call_ping(stacks_ingestion_port)
        .await
        .map_err(|e| cleanup_err(e, &working_dir, redis_port))?;
    let result = metrics
        .get("stacks")
        .unwrap()
        .get("registered_predicates")
        .unwrap();
    assert_eq!(result, 1);

    sleep(Duration::new(1, 0));
    cleanup(&working_dir, redis_port);
    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn prometheus_endpoint_returns_encoded_metrics() -> Result<(), String> {
    let TestSetupResult {
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
    call_register_predicate(&predicate, chainhook_service_port)
        .await
        .map_err(|e| cleanup_err(e, &working_dir, redis_port))?;

    sleep(Duration::new(1, 0));
    let metrics = call_prometheus(prometheus_port)
        .await
        .map_err(|e| cleanup_err(e, &working_dir, redis_port))?;


    // Define expected metric groups with their expected values
    let expected_metrics = [
        // Bitcoin metrics
        ("chainhook_btc_block_evaluation_lag", "0"),
        ("chainhook_btc_canonical_fork_lag", "0"),
        ("chainhook_btc_deregistered_predicates", "0"),
        ("chainhook_btc_highest_block_appended", "0"),
        ("chainhook_btc_highest_block_evaluated", "0"),
        ("chainhook_btc_highest_block_received", "0"),
        ("chainhook_btc_last_block_ingestion_time", "0"),
        ("chainhook_btc_last_reorg_applied_blocks", "0"),
        ("chainhook_btc_last_reorg_rolled_back_blocks", "0"),
        ("chainhook_btc_last_reorg_timestamp", "0"),
        ("chainhook_btc_registered_predicates", "0"),
        
        // Stacks metrics
        ("chainhook_stx_block_evaluation_lag", "0"),
        ("chainhook_stx_canonical_fork_lag", "0"),
        ("chainhook_stx_deregistered_predicates", "0"),
        ("chainhook_stx_highest_block_appended", "1"),
        ("chainhook_stx_highest_block_evaluated", "1"),
        ("chainhook_stx_highest_block_received", "1"),
        ("chainhook_stx_last_reorg_applied_blocks", "0"),
        ("chainhook_stx_last_reorg_rolled_back_blocks", "0"),
        ("chainhook_stx_last_reorg_timestamp", "0"),
        ("chainhook_stx_registered_predicates", "1"),
    ];

    // Verify each metric exists with the expected value
    for (metric_name, expected_value) in expected_metrics {
        // Check the gauge line exists with the correct value
        let gauge_line = format!("{} {}", metric_name, expected_value);
        assert!(
            metrics.contains(&gauge_line),
            "Metric '{}' with value '{}' not found in response", 
            metric_name, 
            expected_value
        );
        
        // Check that the TYPE line exists
        let type_line = format!("# TYPE {} gauge", metric_name);
        assert!(
            metrics.contains(&type_line),
            "Type declaration for '{}' not found", 
            metric_name
        );
        
        // Check that the HELP line exists
        assert!(
            metrics.contains(&format!("# HELP {}", metric_name)),
            "Help text for '{}' not found", 
            metric_name
        );
    }

    // Special case for timestamp which will vary
    assert!(
        metrics.contains("# TYPE chainhook_stx_last_block_ingestion_time gauge") && 
        metrics.contains("# HELP chainhook_stx_last_block_ingestion_time") &&
        metrics.contains("chainhook_stx_last_block_ingestion_time "),
        "Last block ingestion time metric is missing or incomplete"
    );


    println!("metrics: {}", metrics);

    const EXPECTED: &str = "# HELP chainhook_stx_registered_predicates The number of Stacks predicates that have been registered by the Chainhook node.\n# TYPE chainhook_stx_registered_predicates gauge\nchainhook_stx_registered_predicates 1\n";
    assert!(metrics.contains(EXPECTED));

    sleep(Duration::new(1, 0));
    cleanup(&working_dir, redis_port);
    Ok(())
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
#[test_case("/drop_mempool_tx", Method::POST, Some(&json!({})))]
#[test_case("/attachments/new", Method::POST, Some(&json!({})))]
#[test_case("/mined_block", Method::POST, Some(&json!({})))]
#[test_case("/mined_microblock", Method::POST, Some(&json!({})))]
#[tokio::test]
async fn it_responds_200_for_unimplemented_endpoints(
    endpoint: &str,
    method: Method,
    body: Option<&Value>,
) {
    let ingestion_port = get_free_port().unwrap();
    let (_working_dir, _tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        panic!("test failed with error: {e}");
    });
    let config = EventObserverConfig {
        registered_chainhooks: ChainhookStore::new(),
        predicates_config: PredicatesConfig::default(),
        bitcoin_rpc_proxy_enabled: false,
        bitcoind_rpc_username: String::new(),
        bitcoind_rpc_password: String::new(),
        bitcoind_rpc_url: String::new(),
        bitcoin_block_signaling: chainhook_sdk::types::BitcoinBlockSignaling::Stacks(
            StacksNodeConfig {
                rpc_url: String::new(),
                ingestion_port,
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
