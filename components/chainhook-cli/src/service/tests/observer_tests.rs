use std::{sync::mpsc::channel, thread::sleep, time::Duration};

use chainhook_sdk::{
    observer::{start_event_observer, EventObserverConfig},
    types::{BitcoinNetwork, StacksNodeConfig},
    utils::Context,
};
use reqwest::Method;
use serde_json::Value;
use test_case::test_case;

use crate::service::tests::{
    helpers::{
        build_predicates::build_stacks_payload,
        mock_service::{call_observer_svc, call_ping, call_register_predicate, flush_redis},
    },
    setup_stacks_chainhook_test,
};

use super::helpers::{
    build_predicates::get_random_uuid, get_free_port, mock_stacks_node::create_tmp_working_dir,
};

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn ping_endpoint_returns_metrics() {
    let (
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        _,
    ) = setup_stacks_chainhook_test(1, None, None).await;

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

    assert_eq!(metrics.stacks.registered_predicates, 1);
    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
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
    let (working_dir, _tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        panic!("test failed with error: {e}");
    });
    let (observer_commands_tx, observer_commands_rx) = channel();
    let config = EventObserverConfig {
        chainhook_config: None,
        bitcoin_rpc_proxy_enabled: false,
        ingestion_port: ingestion_port,
        bitcoind_rpc_username: format!(""),
        bitcoind_rpc_password: format!(""),
        bitcoind_rpc_url: format!(""),
        bitcoin_block_signaling: chainhook_sdk::types::BitcoinBlockSignaling::Stacks(
            StacksNodeConfig {
                rpc_url: format!(""),
                ingestion_port: ingestion_port,
            },
        ),
        display_logs: false,
        cache_path: working_dir,
        bitcoin_network: BitcoinNetwork::Regtest,
        stacks_network: chainhook_sdk::types::StacksNetwork::Devnet,
        data_handler_tx: None,
    };
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
        ctx,
    )
    .unwrap();

    // ensure the service is started
    let mut attempts = 0;
    loop {
        let url = format!("http://localhost:{ingestion_port}/ping");
        match call_observer_svc(&url, Method::GET, None).await {
            Ok(_) => break,
            Err(e) => {
                if attempts > 3 {
                    panic!("failed to start event observer, {}", e);
                } else {
                    attempts += 1;
                    println!("attmpets {attempts}");
                    sleep(Duration::new(0, 500_000_000));
                }
            }
        }
    }

    let url = format!("http://localhost:{ingestion_port}{endpoint}");
    let response = call_observer_svc(&url, method, body).await.unwrap();
    assert_eq!(response.get("status").unwrap(), &json!(200));
}
