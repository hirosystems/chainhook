use rocket::serde::json::Value as JsonValue;
use rocket::Shutdown;
use std::net::TcpListener;
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

fn build_stacks_payload(
    network: Option<&str>,
    if_this: Option<JsonValue>,
    then_that: Option<JsonValue>,
    filter: Option<JsonValue>,
) -> JsonValue {
    let network = network.unwrap_or("mainnet");
    let if_this = if_this.unwrap_or(json!({"scope":"txid", "equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"}));
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
        "chain": "stacks",
        "uuid": UUID,
        "name": "test",
        "version": 1,
        "networks": {
            network: network_val
        }
    })
}

async fn build_service(port: u16) -> (Receiver<ObserverCommand>, Shutdown) {
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

async fn call_register_predicate(predicate: &JsonValue, port: u16) -> Result<JsonValue, String> {
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
                    "Failed to deserialize response of POST request to localhost:8765/v1/chainhooks: {}",
                    e
                )
            })?;
    Ok(res)
}

async fn test_register_predicate(predicate: JsonValue) -> Result<(), (String, Shutdown)> {
    // perhaps a little janky, we bind to the port 0 to find an open one, then
    // drop the listener to free up that port
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port 0");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let (rx, shutdown) = build_service(port).await;

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
    assert_eq!(result, format!("\"{UUID}\""));
    Ok(())
}

#[test_case("mainnet" ; "mainnet")]
#[test_case("testnet" ; "testnet")]
#[test_case("regtest" ; "regtest")]
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
    let predicate = build_bitcoin_payload(None, None, None, Some(filters));
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
    let predicate = build_stacks_payload(Some(network), None, None, None);
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
// #[test_case(json!({"scope":"contract_deployment", "implement_sip09": 100}); "with scope contract_deployment type implement_sip09")]
// #[test_case(json!({"scope":"contract_deployment", "implement_sip10": 0}); "with scope contract_deployment type implement_sip10")]
#[test_case(json!({"scope":"contract_call","contract_identifier": "SP000000000000000000002Q6VF78.pox","method": "stack-stx"}); "with scope contract_call")]
#[test_case(json!({"scope":"print_event","contract_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09","contains": "vault"}); "with scope print_event")]
#[test_case(json!({"scope":"ft_event","asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.cbtc-token::cbtc","actions": ["burn"]}); "with scope ft_event")]
#[test_case(json!({"scope":"nft_event","asset_identifier": "ST1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.monkey-sip09::monkeys","actions": ["mint", "transfer", "burn"]}); "with scope nft_event")]
#[test_case(json!({"scope":"stx_event","actions": ["transfer", "lock"]}); "with scope stx_event")]
#[test_case(json!({"scope":"txid","equals": "0xfaaac1833dc4883e7ec28f61e35b41f896c395f8d288b1a177155de2abd6052f"}); "with scope txid")]
#[tokio::test]
async fn it_handles_stacks_if_this_predicates(if_this: JsonValue) {
    let predicate = build_stacks_payload(None, Some(if_this), None, None);
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
    let predicate = build_stacks_payload(None, None, Some(then_that), None);
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
    let predicate = build_stacks_payload(None, None, None, Some(filters));
    match test_register_predicate(predicate).await {
        Ok(_) => {}
        Err((e, shutdown)) => {
            shutdown.notify();
            panic!("{e}");
        }
    }
}
