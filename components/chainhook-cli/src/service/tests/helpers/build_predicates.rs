use rocket::serde::json::Value as JsonValue;

pub const DEFAULT_UUID: &str = "4ecc-4ecc-435b-9948-d5eeca1c3ce6";

pub fn get_random_uuid() -> String {
    let mut rng = rand::thread_rng();
    let random_digit: u64 = rand::Rng::gen(&mut rng);
    format!("test-uuid-{random_digit}")
}

pub fn build_bitcoin_payload(
    network: Option<&str>,
    if_this: Option<JsonValue>,
    then_that: Option<JsonValue>,
    filter: Option<JsonValue>,
    uuid: Option<&str>,
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
        "uuid": uuid.unwrap_or(DEFAULT_UUID),
        "name": "test",
        "version": 1,
        "networks": {
            network: network_val
        }
    })
}

pub fn build_stacks_payload(
    network: Option<&str>,
    if_this: Option<JsonValue>,
    then_that: Option<JsonValue>,
    filter: Option<JsonValue>,
    uuid: Option<&str>,
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
        "uuid": uuid.unwrap_or(DEFAULT_UUID),
        "name": "test",
        "version": 1,
        "networks": {
            network: network_val
        }
    })
}
