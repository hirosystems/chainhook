use chainhook_sdk::utils::Context;
use rocket::serde::json::Value as JsonValue;
use rocket::Shutdown;
use std::net::TcpListener;
use std::thread::sleep;
use std::time::Duration;
use test_case::test_case;

use chainhook_sdk::observer::ObserverCommand;

use self::helpers::build_predicates::{build_bitcoin_payload, build_stacks_payload, DEFAULT_UUID};
use self::helpers::mock_bitcoin_rpc::mock_bitcoin_rpc;
use self::helpers::mock_service::{flush_redis, start_chainhook_service, start_redis};
use self::helpers::mock_stacks_node::{
    create_tmp_working_dir, mine_burn_block, mine_stacks_block, write_stacks_blocks_to_tsv,
};
use crate::scan::stacks::consolidate_local_stacks_chainstate_using_csv;
use crate::service::tests::helpers::build_predicates::get_random_uuid;
use crate::service::tests::helpers::get_free_port;
use crate::service::tests::helpers::mock_service::{
    build_predicate_api_server, call_register_predicate, get_chainhook_config, get_predicate_status,
};
use crate::service::PredicateStatus;

mod helpers;

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

fn assert_confirmed_expiration_status(status: PredicateStatus) {
    match status {
        PredicateStatus::ConfirmedExpiration(_) => {}
        _ => panic!("expected ConfirmedExpiration status, found {:?}", status),
    }
}
fn assert_unconfirmed_expiration_status(status: PredicateStatus) {
    match status {
        PredicateStatus::UnconfirmedExpiration(_) => {}
        _ => panic!("expected UnconfirmedExpiration status, found {:?}", status),
    }
}

fn assert_streaming_status(status: PredicateStatus) {
    match status {
        PredicateStatus::Streaming(_) => {}
        _ => panic!("expected Streaming status, found {:?}", status),
    }
}

fn assert_interrupted_status(status: PredicateStatus) {
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

#[test_case(5, 0, Some(1), Some(3) => using assert_confirmed_expiration_status; "predicate_end_block lower than starting_chain_tip ends with ConfirmedExpiration status")]
#[test_case(5, 0, Some(1), None => using assert_streaming_status; "no predicate_end_block ends with Streaming status")]
#[test_case(3, 0, Some(1), Some(5) => using assert_streaming_status; "predicate_end_block greater than chain_tip ends with Streaming status")]
#[test_case(5, 3, Some(1), Some(7) => using assert_unconfirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining until end_block ends with UnconfirmedExpiration status")]
#[test_case(0, 0, None, None => using assert_interrupted_status; "ommitting start_block ends with Interrupted status")]
#[tokio::test]
async fn test_stacks_predicate_status_is_updated(
    starting_chain_tip: u64,
    blocks_to_mine: u64,
    predicate_start_block: Option<u64>,
    predicate_end_block: Option<u64>,
) -> PredicateStatus {
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

    let (working_dir, tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    let uuid = &get_random_uuid();

    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };

    write_stacks_blocks_to_tsv(starting_chain_tip, &tsv_dir).unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    let predicate = build_stacks_payload(
        Some("devnet"),
        Some(json!({"scope":"block_height", "lower_than": 100})),
        None,
        Some(json!({"start_block": predicate_start_block, "end_block": predicate_end_block})),
        Some(uuid),
    );

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

    start_chainhook_service(config, chainhook_service_port, &ctx)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

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
        .await;
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
    result
}

#[test_case(5, 1, Some(1), Some(3) => using assert_unconfirmed_expiration_status; "predicate_end_block lower than starting_chain_tip with predicate_end_block confirmations < CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with UnconfirmedExpiration status")]
#[test_case(10, 1, Some(1), Some(3) => using assert_confirmed_expiration_status; "predicate_end_block lower than starting_chain_tip with predicate_end_block confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with ConfirmedExpiration status")]
#[test_case(1, 3, Some(1), Some(3) => using assert_unconfirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining blocks so that predicate_end_block confirmations < CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with UnconfirmedExpiration status")]
#[test_case(3, 7, Some(1), Some(4) => using assert_confirmed_expiration_status; "predicate_end_block greater than starting_chain_tip and mining blocks so that predicate_end_block confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH ends with ConfirmedExpiration status")]
#[test_case(0, 0, None, None => using assert_interrupted_status; "ommitting start_block ends with Interrupted status")]
#[tokio::test]
async fn test_bitcoin_predicate_status_is_updated(
    starting_chain_tip: u64,
    blocks_to_mine: u64,
    predicate_start_block: Option<u64>,
    predicate_end_block: Option<u64>,
) -> PredicateStatus {
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

    let (working_dir, tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        flush_redis(redis_port);
        redis_process.kill().unwrap();
        panic!("test failed with error: {e}");
    });

    let uuid = &get_random_uuid();

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

    let predicate = build_bitcoin_payload(
        Some("regtest"),
        Some(json!({"scope":"block"})),
        None,
        Some(json!({"start_block": predicate_start_block, "end_block": predicate_end_block})),
        Some(uuid),
    );

    let config = get_chainhook_config(
        redis_port,
        chainhook_service_port,
        stacks_rpc_port,
        stacks_ingestion_port,
        bitcoin_rpc_port,
        &working_dir,
        &tsv_dir,
    );

    start_chainhook_service(config, chainhook_service_port, &ctx)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            flush_redis(redis_port);
            redis_process.kill().unwrap();
            panic!("test failed with error: {e}");
        });

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
            i + starting_chain_tip,
        )
        .await;
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
    result
}
