use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::{mpsc::Sender, Arc, Mutex},
};

use chainhook_sdk::{
    chainhooks::types::{ChainhookInstance, ChainhookSpecificationNetworkMap},
    observer::ObserverCommand,
    utils::Context,
};
use hiro_system_kit::slog;
use redis::{Commands, Connection};
use rocket::serde::json::{json, Json, Value as JsonValue};
use rocket::State;
use rocket::{
    config::{self, Config, LogLevel},
    Shutdown,
};
use rocket_okapi::{okapi::openapi3::OpenApi, openapi, openapi_get_routes_spec};
use std::error::Error;

use crate::config::PredicatesApiConfig;

use super::{open_readwrite_predicates_db_conn, PredicateStatus};

pub async fn start_predicate_api_server(
    api_config: PredicatesApiConfig,
    observer_commands_tx: Sender<ObserverCommand>,
    ctx: Context,
) -> Result<Shutdown, Box<dyn Error + Send + Sync>> {
    let log_level = LogLevel::Off;

    let mut shutdown_config = config::Shutdown::default();
    shutdown_config.ctrlc = false;
    shutdown_config.grace = 1;
    shutdown_config.mercy = 1;

    let control_config = Config {
        port: api_config.http_port,
        workers: 1,
        address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        keep_alive: 5,
        temp_dir: std::env::temp_dir().into(),
        log_level,
        cli_colors: false,
        shutdown: shutdown_config,
        ..Config::default()
    };

    let (routes, _) = get_routes_spec();

    let background_job_tx_mutex = Arc::new(Mutex::new(observer_commands_tx.clone()));

    let ctx_cloned = ctx.clone();

    let ignite = rocket::custom(control_config)
        .manage(background_job_tx_mutex)
        .manage(api_config)
        .manage(ctx_cloned)
        .mount("/", routes)
        .ignite()
        .await?;

    let predicate_api_shutdown = ignite.shutdown();

    let _ = std::thread::spawn(move || {
        let _ = hiro_system_kit::nestable_block_on(ignite.launch());
    });
    Ok(predicate_api_shutdown)
}

#[openapi(tag = "Health Check")]
#[get("/ping")]
fn handle_ping(ctx: &State<Context>) -> Json<JsonValue> {
    ctx.try_log(|logger| slog::info!(logger, "Handling HTTP GET /ping"));
    Json(json!({
        "status": 200,
        "result": "chainhook service up and running",
    }))
}

#[openapi(tag = "Managing Predicates")]
#[get("/v1/chainhooks", format = "application/json")]
fn handle_get_predicates(
    api_config: &State<PredicatesApiConfig>,
    ctx: &State<Context>,
) -> Json<JsonValue> {
    ctx.try_log(|logger| slog::info!(logger, "Handling HTTP GET /v1/chainhooks"));
    match open_readwrite_predicates_db_conn(api_config) {
        Ok(mut predicates_db_conn) => {
            let predicates = match get_entries_from_predicates_db(&mut predicates_db_conn, &ctx) {
                Ok(predicates) => predicates,
                Err(e) => {
                    ctx.try_log(|logger| slog::warn!(logger, "unable to retrieve predicates: {e}"));
                    return Json(json!({
                        "status": 500,
                        "message": "unable to retrieve predicates",
                    }));
                }
            };

            let serialized_predicates = predicates
                .iter()
                .map(|(p, s)| serialized_predicate_with_status(p, s))
                .collect::<Vec<_>>();

            Json(json!({
                "status": 200,
                "result": serialized_predicates
            }))
        }
        Err(e) => Json(json!({
            "status": 500,
            "message": e,
        })),
    }
}

#[openapi(tag = "Managing Predicates")]
#[post("/v1/chainhooks", format = "application/json", data = "<predicate>")]
fn handle_create_predicate(
    predicate: Result<Json<ChainhookSpecificationNetworkMap>, rocket::serde::json::Error>,
    api_config: &State<PredicatesApiConfig>,
    background_job_tx: &State<Arc<Mutex<Sender<ObserverCommand>>>>,
    ctx: &State<Context>,
) -> Json<JsonValue> {
    ctx.try_log(|logger| slog::info!(logger, "Handling HTTP POST /v1/chainhooks"));
    let predicate = match predicate {
        Err(e) => {
            return Json(json!({
                "status": 422,
                "error": e.to_string(),
            }))
        }
        Ok(predicate) => {
            let predicate = predicate.into_inner();
            if let Err(e) = predicate.validate() {
                return Json(json!({
                    "status": 422,
                    "error": e,
                }));
            }
            predicate
        }
    };

    let predicate_uuid = predicate.get_uuid().to_string();

    if let Ok(mut predicates_db_conn) = open_readwrite_predicates_db_conn(api_config) {
        match get_entry_from_predicates_db(
            &ChainhookInstance::either_stx_or_btc_key(&predicate_uuid),
            &mut predicates_db_conn,
            &ctx,
        ) {
            Ok(Some(_)) => {
                return Json(json!({
                    "status": 409,
                    "error": "Predicate uuid already in use",
                }))
            }
            _ => {}
        }
    }

    let background_job_tx = background_job_tx.inner();
    match background_job_tx.lock() {
        Ok(tx) => {
            let _ = tx.send(ObserverCommand::RegisterPredicate(predicate));
        }
        _ => {}
    };

    Json(json!({
        "status": 200,
        "result": predicate_uuid,
    }))
}

#[openapi(tag = "Managing Predicates")]
#[get("/v1/chainhooks/<predicate_uuid>", format = "application/json")]
fn handle_get_predicate(
    predicate_uuid: String,
    api_config: &State<PredicatesApiConfig>,
    ctx: &State<Context>,
) -> Json<JsonValue> {
    ctx.try_log(|logger| {
        slog::info!(
            logger,
            "Handling HTTP GET /v1/chainhooks/{}",
            predicate_uuid
        )
    });

    match open_readwrite_predicates_db_conn(api_config) {
        Ok(mut predicates_db_conn) => {
            let (predicate, status) = match get_entry_from_predicates_db(
                &ChainhookInstance::either_stx_or_btc_key(&predicate_uuid),
                &mut predicates_db_conn,
                &ctx,
            ) {
                Ok(Some(predicate_with_status)) => predicate_with_status,
                _ => {
                    return Json(json!({
                        "status": 404,
                    }))
                }
            };
            let result = serialized_predicate_with_status(&predicate, &status);
            Json(json!({
                "status": 200,
                "result": result
            }))
        }
        Err(e) => Json(json!({
            "status": 500,
            "message": e,
        })),
    }
}

#[openapi(tag = "Managing Predicates")]
#[delete("/v1/chainhooks/stacks/<predicate_uuid>", format = "application/json")]
fn handle_delete_stacks_predicate(
    predicate_uuid: String,
    background_job_tx: &State<Arc<Mutex<Sender<ObserverCommand>>>>,
    ctx: &State<Context>,
) -> Json<JsonValue> {
    ctx.try_log(|logger| {
        slog::info!(
            logger,
            "Handling HTTP DELETE /v1/chainhooks/stacks/{}",
            predicate_uuid
        )
    });

    let background_job_tx = background_job_tx.inner();
    match background_job_tx.lock() {
        Ok(tx) => {
            let _ = tx.send(ObserverCommand::DeregisterStacksPredicate(predicate_uuid));
        }
        _ => {}
    };

    Json(json!({
        "status": 200,
        "result": "Ok",
    }))
}

#[openapi(tag = "Managing Predicates")]
#[delete("/v1/chainhooks/bitcoin/<predicate_uuid>", format = "application/json")]
fn handle_delete_bitcoin_predicate(
    predicate_uuid: String,
    background_job_tx: &State<Arc<Mutex<Sender<ObserverCommand>>>>,
    ctx: &State<Context>,
) -> Json<JsonValue> {
    ctx.try_log(|logger| {
        slog::info!(
            logger,
            "Handling HTTP DELETE /v1/chainhooks/bitcoin/{}",
            predicate_uuid
        )
    });

    let background_job_tx = background_job_tx.inner();
    match background_job_tx.lock() {
        Ok(tx) => {
            let _ = tx.send(ObserverCommand::DeregisterBitcoinPredicate(predicate_uuid));
        }
        _ => {}
    };

    Json(json!({
        "status": 200,
        "result": "Ok",
    }))
}

pub fn get_entry_from_predicates_db(
    predicate_key: &str,
    predicate_db_conn: &mut Connection,
    _ctx: &Context,
) -> Result<Option<(ChainhookInstance, PredicateStatus)>, String> {
    let entry: HashMap<String, String> = predicate_db_conn.hgetall(predicate_key).map_err(|e| {
        format!(
            "unable to load chainhook associated with key {}: {}",
            predicate_key,
            e.to_string()
        )
    })?;

    let encoded_spec = match entry.get("specification") {
        None => return Ok(None),
        Some(payload) => payload,
    };

    let spec = ChainhookInstance::deserialize_specification(&encoded_spec)?;

    let encoded_status = match entry.get("status") {
        None => Err(format!(
            "found predicate specification with no status for predicate {}",
            predicate_key
        )),
        Some(payload) => Ok(payload),
    }?;

    let status = serde_json::from_str(&encoded_status).map_err(|e| format!("{}", e.to_string()))?;

    Ok(Some((spec, status)))
}

pub fn get_entries_from_predicates_db(
    predicate_db_conn: &mut Connection,
    ctx: &Context,
) -> Result<Vec<(ChainhookInstance, PredicateStatus)>, String> {
    let chainhooks_to_load: Vec<String> = predicate_db_conn
        .scan_match(ChainhookInstance::either_stx_or_btc_key("*"))
        .map_err(|e| format!("unable to connect to redis: {}", e.to_string()))?
        .into_iter()
        .collect();

    let mut predicates = vec![];
    for predicate_key in chainhooks_to_load.iter() {
        let chainhook = match get_entry_from_predicates_db(predicate_key, predicate_db_conn, ctx) {
            Ok(Some((spec, status))) => (spec, status),
            Ok(None) => {
                warn!(
                    ctx.expect_logger(),
                    "unable to load chainhook associated with key {}", predicate_key,
                );
                continue;
            }
            Err(e) => {
                error!(
                    ctx.expect_logger(),
                    "unable to load chainhook associated with key {}: {}",
                    predicate_key,
                    e.to_string()
                );
                continue;
            }
        };
        predicates.push(chainhook);
    }
    Ok(predicates)
}

pub fn load_predicates_from_redis(
    config: &crate::config::Config,
    ctx: &Context,
) -> Result<Vec<(ChainhookInstance, PredicateStatus)>, String> {
    let redis_uri: &str = config.expected_api_database_uri();
    let client = redis::Client::open(redis_uri)
        .map_err(|e| format!("unable to connect to redis: {}", e.to_string()))?;
    let mut predicate_db_conn = client
        .get_connection()
        .map_err(|e| format!("unable to connect to redis: {}", e.to_string()))?;
    get_entries_from_predicates_db(&mut predicate_db_conn, ctx)
}

pub fn document_predicate_api_server() -> Result<String, String> {
    let (_, spec) = get_routes_spec();
    let json_spec = serde_json::to_string_pretty(&spec)
        .map_err(|e| format!("failed to serialize openapi spec: {}", e.to_string()))?;
    Ok(json_spec)
}

pub fn get_routes_spec() -> (Vec<rocket::Route>, OpenApi) {
    openapi_get_routes_spec![
        handle_ping,
        handle_get_predicates,
        handle_get_predicate,
        handle_create_predicate,
        handle_delete_bitcoin_predicate,
        handle_delete_stacks_predicate
    ]
}

fn serialized_predicate_with_status(
    predicate: &ChainhookInstance,
    status: &PredicateStatus,
) -> JsonValue {
    match (predicate, status) {
        (ChainhookInstance::Stacks(spec), status) => json!({
            "chain": "stacks",
            "uuid": spec.uuid,
            "network": spec.network,
            "predicate": spec.predicate,
            "status": status,
            "enabled": spec.enabled,
        }),
        (ChainhookInstance::Bitcoin(spec), status) => json!({
            "chain": "bitcoin",
            "uuid": spec.uuid,
            "network": spec.network,
            "predicate": spec.predicate,
            "status": status,
            "enabled": spec.enabled,
        }),
    }
}
