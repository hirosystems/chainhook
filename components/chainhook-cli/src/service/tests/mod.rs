use chainhook_sdk::chainhooks::types::{
    ChainhookFullSpecification, ChainhookSpecification, StacksChainhookFullSpecification,
};
use chainhook_sdk::types::{Chain, StacksNetwork};
use chainhook_sdk::utils::Context;
use rocket::serde::json::Value as JsonValue;
use rocket::Shutdown;
use std::fs::{self};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Child;
use std::thread::sleep;
use std::time::Duration;
use test_case::test_case;

use chainhook_sdk::observer::ObserverCommand;

use self::helpers::build_predicates::{build_bitcoin_payload, build_stacks_payload, DEFAULT_UUID};
use self::helpers::mock_bitcoin_rpc::mock_bitcoin_rpc;
use self::helpers::mock_service::{
    call_deregister_predicate, filter_predicate_status_from_all_predicates, flush_redis,
    start_chainhook_service, start_redis,
};
use self::helpers::mock_stacks_node::{
    create_tmp_working_dir, mine_burn_block, mine_stacks_block, write_stacks_blocks_to_tsv,
};
use crate::scan::stacks::consolidate_local_stacks_chainstate_using_csv;
use crate::service::tests::helpers::build_predicates::get_random_uuid;
use crate::service::tests::helpers::get_free_port;
use crate::service::tests::helpers::mock_service::{
    build_predicate_api_server, call_get_predicate, call_register_predicate, get_chainhook_config,
    get_predicate_status,
};
use crate::service::tests::helpers::mock_stacks_node::create_burn_fork_at;
use crate::service::{PredicateStatus, PredicateStatus::*, ScanningData, StreamingData};

use super::http_api::document_predicate_api_server;
use super::{update_predicate_spec, update_predicate_status};

pub mod helpers;
mod observer_tests;

async fn test_register_predicate(predicate: JsonValue) -> Result<(), (String, Shutdown)> {
    // perhaps a little janky, we bind to the port 0 to find an open one, then
    // drop the listener to free up that port
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port 0");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let (rx, shutdown) = build_predicate_api_server(port).await;

    let moved_shutdown = shutdown.clone();
    let res = call_register_predicate(&predicate, port)
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
    assert_eq!(result, format!("\"{DEFAULT_UUID}\""));
    Ok(())
}

#[test_case("mainnet" ; "mainnet")]
#[test_case("testnet" ; "testnet")]
#[test_case("regtest" ; "regtest")]
#[tokio::test]
async fn it_handles_bitcoin_predicates_with_network(network: &str) {
    let predicate = build_bitcoin_payload(Some(network), None, None, None, None);
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
#[test_case(json!({"scope": "outputs","descriptor": {"expression": "a descriptor", "range": [0,3]}}) ; "with scope outputs type descriptor")]
#[test_case(json!({"scope": "stacks_protocol","operation": "stacker_rewarded"}) ; "with scope stacks_protocol operation stacker_rewarded")]
#[test_case(json!({"scope": "stacks_protocol","operation": "block_committed"}) ; "with scope stacks_protocol operation block_committed")]
#[test_case(json!({"scope": "stacks_protocol","operation": "leader_registered"}) ; "with scope stacks_protocol operation leader_registered")]
#[test_case(json!({"scope": "stacks_protocol","operation": "stx_transferred"}) ; "with scope stacks_protocol operation stx_transferred")]
#[test_case(json!({"scope": "stacks_protocol","operation": "stx_locked"}) ; "with scope stacks_protocol operation stx_locked")]
#[test_case(json!({"scope": "ordinals_protocol","operation": "inscription_feed"}) ; "with scope ordinals_protocol operation inscription_feed")]
#[tokio::test]
async fn it_handles_bitcoin_if_this_predicates(if_this: JsonValue) {
    let predicate = build_bitcoin_payload(None, Some(if_this), None, None, None);
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
#[tokio::test]
async fn it_handles_bitcoin_then_that_predicates(then_that: JsonValue) {
    let predicate = build_bitcoin_payload(None, None, Some(then_that), None, None);
    match test_register_predicate(predicate).await {
        Ok(_) => {}
        Err((e, shutdown)) => {
            shutdown.notify();
            panic!("{e}");
        }
    }
}

#[test_case(json!({"blocks": [0, 1, 2],"start_block": 0,"end_block": 0,"expire_after_occurrence": 0,"include_proof": true,"include_inputs": true,"include_outputs": true,"include_witness": true}) ; "all filters")]
#[test_case(json!({"blocks": [0, 1, 2]}) ; "blocks filter")]
#[test_case(json!({"start_block": 0}) ; "start_block filter")]
#[test_case(json!({"end_block": 0}) ; "end_block filter")]
#[test_case(json!({"expire_after_occurrence": 0}) ; "expire_after_occurrence filter")]
#[test_case(json!({"include_proof": true}) ; "include_proof filter")]
#[test_case(json!({"include_inputs": true}) ; "include_inputs filter")]
#[test_case(json!({"include_outputs": true}) ; "include_outputs filter")]
#[test_case(json!({"include_witness": true}) ; "include_witness filter")]
#[tokio::test]
async fn it_handles_bitcoin_predicates_with_filters(filters: JsonValue) {
    let predicate = build_bitcoin_payload(None, None, None, Some(filters), None);
    match test_register_predicate(predicate).await {
        Ok(_) => {}
        Err((e, shutdown)) => {
            shutdown.notify();
            panic!("{e}");
        }
    }
}

#[test_case("mainnet" ; "mainnet")]
#[test_case("testnet" ; "testnet")]
#[test_case("devnet" ; "devnet")]
#[test_case("simnet" ; "simnet")]
#[tokio::test]
async fn it_handles_stacks_predicates_with_network(network: &str) {
    let predicate = build_stacks_payload(Some(network), None, None, None, None);
    match test_register_predicate(predicate).await {
        Ok(_) => {}
        Err((e, shutdown)) => {
            shutdown.notify();
            panic!("{e}");
        }
    }
}

#[test_case(json!({"scope":"block_height", "equals": 100}); "with scope block_height equals match")]
#[test_case(json!({"scope":"block_height", "higher_than": 100}); "with scope block_height higher_than match")]
#[test_case(json!({"scope":"block_height", "lower_than": 100}); "with scope block_height lower_than match")]
#[test_case(json!({"scope":"block_height", "between": [100,102]}); "with scope block_height between match")]
#[test_case(json!({"scope":"contract_deployment", "deployer": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM"}); "with scope contract_deployment type deployer")]
#[test_case(json!({"scope":"contract_deployment", "deployer": "*"}); "with scope contract_deployment type deployer wildcard")]
#[test_case(json!({"scope":"contract_deployment", "implement_trait": "sip09"}); "with scope contract_deployment type implement_trait sip09")]
#[test_case(json!({"scope":"contract_deployment", "implement_trait": "sip10"}); "with scope contract_deployment type implement_trait sip10")]
#[test_case(json!({"scope":"contract_deployment", "implement_trait": "*"}); "with scope contract_deployment type implement_trait and wildcard trait")]
#[test_case(json!({"scope":"contract_call","contract_identifier": "SP000000000000000000002Q6VF78.pox","method": "stack-stx"}); "with scope contract_call")]
#[test_case(json!({"scope":"print_event","contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09","contains": "vault"}); "with scope print_event both fields")]
#[test_case(json!({"scope":"print_event","contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09", "contains": "*"}); "with scope print_event wildcard conatins")]
#[test_case(json!({"scope":"print_event","contract_identifier": "*", "contains": "vault"}); "with scope print_event wildcard contract_identifier")]
#[test_case(json!({"scope":"print_event", "contract_identifier": "*", "contains": "*"}); "with scope print_event wildcard both fields")]
#[test_case(json!({"scope":"print_event", "contract_identifier": "*", "matches_regex": "(some)|(value)"}); "with scope print_event and matching_rule regex")]
#[test_case(json!({"scope":"ft_event","asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.cbtc-token::cbtc","actions": ["burn"]}); "with scope ft_event")]
#[test_case(json!({"scope":"nft_event","asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys","actions": ["mint", "transfer", "burn"]}); "with scope nft_event")]
#[test_case(json!({"scope":"stx_event","actions": ["transfer", "lock"]}); "with scope stx_event")]
#[test_case(json!({"scope":"txid","equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"}); "with scope txid")]
#[tokio::test]
async fn it_handles_stacks_if_this_predicates(if_this: JsonValue) {
    let predicate = build_stacks_payload(None, Some(if_this), None, None, None);
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
#[tokio::test]
async fn it_handles_stacks_then_that_predicates(then_that: JsonValue) {
    let predicate = build_stacks_payload(None, None, Some(then_that), None, None);
    match test_register_predicate(predicate).await {
        Ok(_) => {}
        Err((e, shutdown)) => {
            shutdown.notify();
            panic!("{e}");
        }
    }
}

#[test_case(json!({"blocks": [0, 1, 2], "start_block": 0,"end_block": 0,"expire_after_occurrence": 0,"capture_all_events": true,"decode_clarity_values": true}) ; "all filters")]
#[test_case(json!({"blocks": [0, 1, 2]}) ; "blocks filter")]
#[test_case(json!({"start_block": 0}) ; "start_block filter")]
#[test_case(json!({"end_block": 0}) ; "end_block filter")]
#[test_case(json!({"expire_after_occurrence": 0}) ; "expire_after_occurrence filter")]
#[test_case(json!({"capture_all_events": true}) ; "capture_all_events filter")]
#[test_case(json!({"decode_clarity_values": true}) ; "decode_clarity_values filter")]
#[tokio::test]
async fn it_handles_stacks_predicates_with_filters(filters: JsonValue) {
    let predicate = build_stacks_payload(None, None, None, Some(filters), None);
    match test_register_predicate(predicate).await {
        Ok(_) => {}
        Err((e, shutdown)) => {
            shutdown.notify();
            panic!("{e}");
        }
    }
}

fn assert_confirmed_expiration_status(
    (status, expected_evaluations, expected_occurrences): (
        PredicateStatus,
        Option<u64>,
        Option<u64>,
    ),
) {
    match status {
        PredicateStatus::ConfirmedExpiration(data) => {
            if let Some(expected) = expected_evaluations {
                assert_eq!(
                    data.number_of_blocks_evaluated, expected,
                    "incorrect number of blocks evaluated"
                );
            }
            if let Some(expected) = expected_occurrences {
                assert_eq!(
                    data.number_of_times_triggered, expected,
                    "incorrect number of predicates triggered"
                );
            }
        }
        _ => panic!("expected ConfirmedExpiration status, found {:?}", status),
    }
}
fn assert_unconfirmed_expiration_status(
    (status, expected_evaluations, expected_occurrences): (
        PredicateStatus,
        Option<u64>,
        Option<u64>,
    ),
) {
    match status {
        PredicateStatus::UnconfirmedExpiration(data) => {
            if let Some(expected) = expected_evaluations {
                assert_eq!(
                    data.number_of_blocks_evaluated, expected,
                    "incorrect number of blocks evaluated"
                );
            }
            if let Some(expected) = expected_occurrences {
                assert_eq!(
                    data.number_of_times_triggered, expected,
                    "incorrect number of predicates triggered"
                );
            }
        }
        _ => panic!("expected UnconfirmedExpiration status, found {:?}", status),
    }
}

fn assert_streaming_status(
    (status, expected_evaluations, expected_occurrences): (
        PredicateStatus,
        Option<u64>,
        Option<u64>,
    ),
) {
    match status {
        PredicateStatus::Streaming(data) => {
            if let Some(expected) = expected_evaluations {
                assert_eq!(
                    data.number_of_blocks_evaluated, expected,
                    "incorrect number of blocks evaluated"
                );
            }
            if let Some(expected) = expected_occurrences {
                assert_eq!(
                    data.number_of_times_triggered, expected,
                    "incorrect number of predicates triggered"
                );
            }
        }
        _ => panic!("expected Streaming status, found {:?}", status),
    }
}

fn assert_interrupted_status((status, _, _): (PredicateStatus, Option<u64>, Option<u64>)) {
    match status {
        PredicateStatus::Interrupted(_) => {}
        _ => panic!("expected Interrupted status, found {:?}", status),
    }
}

fn setup_chainhook_service_ports() -> Result<(u16, u16, u16, u16, u16), String> {
    let redis_port = get_free_port()?;
    let chainhook_service_port = get_free_port()?;
    let stacks_rpc_port = get_free_port()?;
    let stacks_ingestion_port = get_free_port()?;
    let bitcoin_rpc_port = get_free_port()?;
    Ok((
        redis_port,
        chainhook_service_port,
        stacks_rpc_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    ))
}

async fn await_new_scanning_status_complete(
    uuid: &str,
    chainhook_service_port: u16,
) -> Result<(), String> {
    let mut attempts = 0;
    loop {
        match get_predicate_status(uuid, chainhook_service_port).await? {
            PredicateStatus::New | PredicateStatus::Scanning(_) => {
                attempts += 1;
                if attempts == 10 {
                    return Err(format!("predicate stuck in new/scanning status"));
                }
                sleep(Duration::new(1, 0));
            }
            _ => break Ok(()),
        }
    }
}

async fn setup_stacks_chainhook_test(
    starting_chain_tip: u64,
    redis_seed: Option<(StacksChainhookFullSpecification, PredicateStatus)>,
    startup_predicates: Option<Vec<ChainhookFullSpecification>>,
) -> (Child, String, u16, u16, u16, u16) {
    let (
        redis_port,
        chainhook_service_port,
        stacks_rpc_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    ) = setup_chainhook_service_ports().unwrap_or_else(|e| panic!("test failed with error: {e}"));

    let mut redis_process = start_redis(redis_port)
        .await
        .unwrap_or_else(|e| panic!("test failed with error: {e}"));
    flush_redis(redis_port);

    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };

    if let Some((predicate, status)) = redis_seed {
        let client = redis::Client::open(format!("redis://localhost:{redis_port}/"))
            .unwrap_or_else(|e| {
                flush_redis(redis_port);
                redis_process.kill().unwrap();
                panic!("test failed with error: {e}");
            });
        let mut connection = client.get_connection().unwrap_or_else(|e| {
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
        let stacks_spec = predicate
            .into_selected_network_specification(&StacksNetwork::Devnet)
            .unwrap_or_else(|e| {
                flush_redis(redis_port);
                redis_process.kill().unwrap();
                panic!("test failed with error: {e}");
            });

        let spec = ChainhookSpecification::Stacks(stacks_spec);
        update_predicate_spec(&spec.key(), &spec, &mut connection, &ctx);
        update_predicate_status(&spec.key(), status, &mut connection, &ctx);
    }

    let (working_dir, tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    write_stacks_blocks_to_tsv(starting_chain_tip, &tsv_dir).unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    let mut config = get_chainhook_config(
        redis_port,
        chainhook_service_port,
        stacks_rpc_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
        &working_dir,
        &tsv_dir,
    );

    consolidate_local_stacks_chainstate_using_csv(&mut config, &ctx)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    start_chainhook_service(config, chainhook_service_port, startup_predicates, &ctx)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    (
        redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    )
}

#[test_case(5, 0, Some(1), Some(3), Some(3), Some(3) => using assert_confirmed_expiration_status; "predicate_end_block lower than starting_chain_tip ends with ConfirmedExpiration status")]
#[test_case(5, 0, Some(1), None, Some(5), Some(5) => using assert_streaming_status; "no predicate_end_block ends with Streaming status")]
#[test_case(3, 0, Some(1), Some(5), Some(3), Some(3) => using assert_streaming_status; "predicate_end_block greater than chain_tip ends with Streaming status")]
#[test_case(5, 4, Some(1), Some(7), Some(9), Some(7) => using assert_unconfirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining until end_block ends with UnconfirmedExpiration status")]
#[test_case(1, 3, Some(1), Some(3), Some(4), Some(3) => using assert_unconfirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining blocks so that predicate_end_block confirmations < CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with UnconfirmedExpiration status")]
#[test_case(3, 7, Some(1), Some(4), Some(9), Some(4) => using assert_confirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining blocks so that predicate_end_block confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with ConfirmedExpiration status")]
#[test_case(0, 0, None, None, None, None => using assert_interrupted_status; "ommitting start_block ends with Interrupted status")]
#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn test_stacks_predicate_status_is_updated(
    starting_chain_tip: u64,
    blocks_to_mine: u64,
    predicate_start_block: Option<u64>,
    predicate_end_block: Option<u64>,
    expected_evaluations: Option<u64>,
    expected_occurrences: Option<u64>,
) -> (PredicateStatus, Option<u64>, Option<u64>) {
    let (
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        _,
    ) = setup_stacks_chainhook_test(starting_chain_tip, None, None).await;

    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(
        Some("devnet"),
        Some(json!({"scope":"block_height", "lower_than": 600})),
        None,
        Some(json!({"start_block": predicate_start_block, "end_block": predicate_end_block})),
        Some(uuid),
    );
    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    await_new_scanning_status_complete(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    for i in 1..blocks_to_mine + 1 {
        mine_stacks_block(
            stacks_ingestion_port,
            i + starting_chain_tip,
            i + starting_chain_tip + 100,
        )
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    }
    sleep(Duration::new(2, 0));
    let result = get_predicate_status(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    let found_predicate_status =
        filter_predicate_status_from_all_predicates(uuid, chainhook_service_port)
            .await
            .unwrap_or_else(|e| {
                std::fs::remove_dir_all(&working_dir).unwrap();
                flush_redis(redis_port);
                redis_process.kill().unwrap();
                panic!("test failed with error: {e}");
            });
    assert_eq!(found_predicate_status, result);

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
    (result, expected_evaluations, expected_occurrences)
}

async fn setup_bitcoin_chainhook_test(
    starting_chain_tip: u64,
) -> (Child, String, u16, u16, u16, u16) {
    let (
        redis_port,
        chainhook_service_port,
        stacks_rpc_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    ) = setup_chainhook_service_ports().unwrap_or_else(|e| panic!("test failed with error: {e}"));

    let mut redis_process = start_redis(redis_port)
        .await
        .unwrap_or_else(|e| panic!("test failed with error: {e}"));

    flush_redis(redis_port);
    let (working_dir, tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };

    let _ = hiro_system_kit::thread_named("Bitcoin rpc service")
        .spawn(move || {
            let future = mock_bitcoin_rpc(bitcoin_rpc_port, starting_chain_tip);
            let _ = hiro_system_kit::nestable_block_on(future);
        })
        .expect("unable to spawn thread");

    let config = get_chainhook_config(
        redis_port,
        chainhook_service_port,
        stacks_rpc_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
        &working_dir,
        &tsv_dir,
    );

    start_chainhook_service(config, chainhook_service_port, None, &ctx)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    (
        redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    )
}

#[test_case(5, 1, Some(1), Some(3), Some(3), Some(3) => using assert_unconfirmed_expiration_status; "predicate_end_block lower than starting_chain_tip with predicate_end_block confirmations < CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with UnconfirmedExpiration status")]
#[test_case(10, 1, Some(1), Some(3), Some(3), Some(3) => using assert_confirmed_expiration_status; "predicate_end_block lower than starting_chain_tip with predicate_end_block confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with ConfirmedExpiration status")]
#[test_case(1, 3, Some(1), Some(3), Some(4), Some(3) => using assert_unconfirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining blocks so that predicate_end_block confirmations < CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with UnconfirmedExpiration status")]
#[test_case(3, 7, Some(1), Some(4), Some(9), Some(4) => using assert_confirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining blocks so that predicate_end_block confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with ConfirmedExpiration status")]
#[test_case(0, 0, None, None, None, None => using assert_interrupted_status; "ommitting start_block ends with Interrupted status")]
#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn test_bitcoin_predicate_status_is_updated(
    starting_chain_tip: u64,
    blocks_to_mine: u64,
    predicate_start_block: Option<u64>,
    predicate_end_block: Option<u64>,
    expected_evaluations: Option<u64>,
    expected_occurrences: Option<u64>,
) -> (PredicateStatus, Option<u64>, Option<u64>) {
    let (
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    ) = setup_bitcoin_chainhook_test(starting_chain_tip).await;

    let uuid = &get_random_uuid();
    let predicate = build_bitcoin_payload(
        Some("regtest"),
        Some(json!({"scope":"block"})),
        None,
        Some(
            json!({"start_block": predicate_start_block, "end_block": predicate_end_block, "include_proof": true}),
        ),
        Some(uuid),
    );

    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    await_new_scanning_status_complete(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    for i in 1..blocks_to_mine + 1 {
        mine_burn_block(
            stacks_ingestion_port,
            bitcoin_rpc_port,
            None,
            i + starting_chain_tip,
        )
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    }
    sleep(Duration::new(2, 0));
    let result = get_predicate_status(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    let found_predicate_status =
        filter_predicate_status_from_all_predicates(uuid, chainhook_service_port)
            .await
            .unwrap_or_else(|e| {
                std::fs::remove_dir_all(&working_dir).unwrap();
                flush_redis(redis_port);
                redis_process.kill().unwrap();
                panic!("test failed with error: {e}");
            });
    assert_eq!(found_predicate_status, result);

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
    (result, expected_evaluations, expected_occurrences)
}

///            
///          ┌─> predicate start block
///          │                               ┌─> reorg, predicate scans from A(3) to B(6)
///          │                               │       ┌─> predicate end block (unconfirmed set)
///  A(1) -> A(2) -> A(3) -> A(4) -> A(5)    │       │                                         ┌─> predicate status confirmed
///                     \ -> B(4) -> B(5) -> B(6) -> B(7) -> B(8) -> B(9) -> B(10) -> B(11) -> B(12)
///                                  
///                         
#[test_case(5, 3, 9, Some(2), Some(7); "ommitting start_block ends with Interrupted status")]
#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn test_bitcoin_predicate_status_is_updated_with_reorg(
    genesis_chain_blocks_to_mine: u64,
    fork_point: u64,
    fork_blocks_to_mine: u64,
    predicate_start_block: Option<u64>,
    predicate_end_block: Option<u64>,
) {
    let starting_chain_tip = 0;
    let (
        mut redis_process,
        working_dir,
        chainhook_service_port,
        redis_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
    ) = setup_bitcoin_chainhook_test(starting_chain_tip).await;

    let uuid = &get_random_uuid();
    let predicate = build_bitcoin_payload(
        Some("regtest"),
        Some(json!({"scope":"block"})),
        None,
        Some(
            json!({"start_block": predicate_start_block, "end_block": predicate_end_block, "include_proof": true}),
        ),
        Some(uuid),
    );

    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    let genesis_branch_key = '0';
    let first_block_mined_height = starting_chain_tip + 1;
    let last_block_mined_height = genesis_chain_blocks_to_mine + first_block_mined_height;
    for block_height in first_block_mined_height..last_block_mined_height {
        mine_burn_block(
            stacks_ingestion_port,
            bitcoin_rpc_port,
            Some(genesis_branch_key),
            block_height,
        )
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    }

    sleep(Duration::new(2, 0));
    let status = get_predicate_status(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    assert_streaming_status((status, None, None));

    let branch_key = '1';
    let first_fork_block_mined_height = fork_point + 1;
    create_burn_fork_at(
        stacks_ingestion_port,
        bitcoin_rpc_port,
        Some(branch_key),
        first_fork_block_mined_height,
        genesis_branch_key,
        fork_point,
    )
    .await
    .unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    let reorg_point = last_block_mined_height + 1;
    let first_fork_block_mined_height = first_fork_block_mined_height + 1;
    let last_fork_block_mined_height = first_fork_block_mined_height + fork_blocks_to_mine;

    for block_height in first_fork_block_mined_height..last_fork_block_mined_height {
        mine_burn_block(
            stacks_ingestion_port,
            bitcoin_rpc_port,
            Some(branch_key),
            block_height,
        )
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
        if block_height == reorg_point {
            sleep(Duration::new(2, 0));
            let status = get_predicate_status(uuid, chainhook_service_port)
                .await
                .unwrap_or_else(|e| {
                    std::fs::remove_dir_all(&working_dir).unwrap();
                    flush_redis(redis_port);
                    redis_process.kill().unwrap();
                    panic!("test failed with error: {e}");
                });
            assert_streaming_status((status, None, None));
        }
    }

    sleep(Duration::new(2, 0));
    let status = get_predicate_status(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    assert_confirmed_expiration_status((status, None, None));

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}

#[test_case(Chain::Stacks; "for stacks chain")]
#[test_case(Chain::Bitcoin; "for bitcoin chain")]
#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn test_deregister_predicate(chain: Chain) {
    let (mut redis_process, working_dir, chainhook_service_port, redis_port, _, _) = match &chain {
        Chain::Stacks => setup_stacks_chainhook_test(0, None, None).await,
        Chain::Bitcoin => setup_bitcoin_chainhook_test(0).await,
    };

    let uuid = &get_random_uuid();

    let predicate = match &chain {
        Chain::Stacks => build_stacks_payload(
            Some("devnet"),
            Some(json!({"scope":"block_height", "lower_than": 100})),
            None,
            Some(json!({"start_block": 1, "end_block": 2})),
            Some(uuid),
        ),
        Chain::Bitcoin => build_bitcoin_payload(
            Some("regtest"),
            Some(json!({"scope":"block"})),
            None,
            Some(json!({"start_block": 1, "end_block": 2, "include_proof": true})),
            Some(uuid),
        ),
    };

    let _ = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    let result = call_get_predicate(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    assert_eq!(result.get("status"), Some(&json!(200)));

    let result = call_deregister_predicate(&chain, uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    assert_eq!(result.get("status"), Some(&json!(200)));

    let mut attempts = 0;
    loop {
        let result = call_get_predicate(uuid, chainhook_service_port)
            .await
            .unwrap_or_else(|e| {
                std::fs::remove_dir_all(&working_dir).unwrap();
                flush_redis(redis_port);
                redis_process.kill().unwrap();
                panic!("test failed with error: {e}");
            });
        if result.get("status") == Some(&json!(404)) {
            break;
        } else if attempts == 3 {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("predicate was not successfully derigistered");
        } else {
            attempts += 1;
            sleep(Duration::new(1, 0));
        }
    }

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}

#[test_case(New, 6 => using assert_confirmed_expiration_status; "preloaded predicate with new status should get scanned until completion")]
#[test_case(Scanning(ScanningData {
    number_of_blocks_evaluated: 4,
    number_of_blocks_to_scan: 1,
    number_of_times_triggered: 0,
    last_occurrence: None,
    last_evaluated_block_height: 4
}), 6 => using assert_confirmed_expiration_status; "preloaded predicate with scanning status should get scanned until completion")]
#[test_case(Streaming(StreamingData {
    number_of_blocks_evaluated: 4,
    number_of_times_triggered: 0,
    last_occurrence: None,
    last_evaluation: 0,
    last_evaluated_block_height: 4
}), 6 => using assert_confirmed_expiration_status; "preloaded predicate with streaming status and last evaluated height below tip should get scanned until completion")]
#[test_case(Streaming(StreamingData {
    number_of_blocks_evaluated: 5,
    number_of_times_triggered: 0,
    last_occurrence: None,
    last_evaluation: 0,
    last_evaluated_block_height: 5
}), 5 => using assert_streaming_status; "preloaded predicate with streaming status and last evaluated height at tip should be streamed")]
#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn test_restarting_with_saved_predicates(
    starting_status: PredicateStatus,
    starting_chain_tip: u64,
) -> (PredicateStatus, Option<u64>, Option<u64>) {
    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(
        Some("devnet"),
        Some(json!({"scope":"block_height", "lower_than": 100})),
        None,
        Some(json!({"start_block": 1, "end_block": 6})),
        Some(uuid),
    );
    let predicate =
        serde_json::from_value(predicate).expect("failed to set up stacks chanhook spec for test");

    let (mut redis_process, working_dir, chainhook_service_port, redis_port, _, _) =
        setup_stacks_chainhook_test(starting_chain_tip, Some((predicate, starting_status)), None)
            .await;

    await_new_scanning_status_complete(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    sleep(Duration::new(2, 0));
    let result = get_predicate_status(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
    (result, None, None)
}

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn it_allows_specifying_startup_predicate() {
    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(
        Some("devnet"),
        Some(json!({"scope":"block_height", "lower_than": 100})),
        None,
        Some(json!({"start_block": 1, "end_block": 2})),
        Some(uuid),
    );
    let predicate =
        serde_json::from_value(predicate).expect("failed to set up stacks chanhook spec for test");
    let startup_predicate = ChainhookFullSpecification::Stacks(predicate);
    let (mut redis_process, working_dir, chainhook_service_port, redis_port, _, _) =
        setup_stacks_chainhook_test(3, None, Some(vec![startup_predicate])).await;

    await_new_scanning_status_complete(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    sleep(Duration::new(2, 0));
    let result = get_predicate_status(uuid, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
    assert_confirmed_expiration_status((result, None, None));
}

#[tokio::test]
#[cfg_attr(not(feature = "redis_tests"), ignore)]
async fn register_predicate_responds_409_if_uuid_in_use() {
    let uuid = &get_random_uuid();
    let predicate = build_stacks_payload(
        Some("devnet"),
        Some(json!({"scope":"block_height", "lower_than": 100})),
        None,
        Some(json!({"start_block": 1, "end_block": 2})),
        Some(uuid),
    );
    let stacks_spec = serde_json::from_value(predicate.clone())
        .expect("failed to set up stacks chanhook spec for test");
    let startup_predicate = ChainhookFullSpecification::Stacks(stacks_spec);

    let (mut redis_process, working_dir, chainhook_service_port, redis_port, _, _) =
        setup_stacks_chainhook_test(3, None, Some(vec![startup_predicate])).await;

    let result = call_register_predicate(&predicate, chainhook_service_port)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });
    assert_eq!(result.get("status"), Some(&json!(409)));

    std::fs::remove_dir_all(&working_dir).unwrap();
    flush_redis(redis_port);
    redis_process.kill().unwrap();
}

#[test]
fn it_generates_open_api_spec() {
    let new_spec = document_predicate_api_server().unwrap();

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../docs/chainhook-openapi.json");
    let current_spec = fs::read_to_string(path).unwrap();

    assert_eq!(
        current_spec, new_spec,
        "breaking change detected: open api spec has been updated"
    )
}
