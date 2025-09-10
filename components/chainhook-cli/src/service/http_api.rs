use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{mpsc::Sender, Arc, Mutex},
};

use chainhook_sdk::{
    chainhooks::{
        database::PredicatesDatabaseAccess,
        types::{ChainhookInstance, ChainhookSpecificationNetworkMap},
    },
    observer::ObserverCommand,
    try_error, try_info, try_warn,
    utils::Context,
};
use hiro_system_kit::slog;
use rocket::State;
use rocket::{
    config::{self, Config, LogLevel},
    Shutdown,
};
use rocket::{
    http::Status,
    response::status::Custom,
    serde::json::{json, Json, Value as JsonValue},
};
use rocket_okapi::{okapi::openapi3::OpenApi, openapi, openapi_get_routes_spec};
use std::error::Error;

use crate::{config::PredicatesApiConfig, storage::predicates_db::RedisPredicatesDatabaseAccess};

use super::PredicateStatus;

pub async fn start_predicate_api_server(
    api_config: PredicatesApiConfig,
    observer_commands_tx: Sender<ObserverCommand>,
    predicates_database_access: RedisPredicatesDatabaseAccess,
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
        .manage(predicates_database_access)
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

fn success_response(result: JsonValue) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    Ok(Json(json!({
        "status": 200,
        "result": result,
    })))
}

fn error_response(
    message: String,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_error!(ctx, "{message}");
    Err(Custom(
        Status::InternalServerError,
        Json(json!({
            "status": Status::InternalServerError.code,
            "result": message,
        })),
    ))
}

fn user_error_response(
    message: String,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_warn!(ctx, "{message}");
    Err(Custom(
        Status::UnprocessableEntity,
        Json(json!({
            "status": Status::UnprocessableEntity.code,
            "result": message,
        })),
    ))
}

fn not_found_response() -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    Err(Custom(
        Status::NotFound,
        Json(json!({
            "status": Status::NotFound.code,
            "result": "Not Found",
        })),
    ))
}

#[openapi(tag = "Health Check")]
#[get("/ping")]
fn handle_ping(ctx: &State<Context>) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_info!(ctx, "Handling HTTP GET /ping");
    success_response(json!("chainhook service up and running"))
}

#[openapi(tag = "Managing Predicates")]
#[get("/v1/chainhooks", format = "application/json")]
fn handle_get_predicates(
    predicates_database_access: &State<RedisPredicatesDatabaseAccess>,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_info!(ctx, "Handling HTTP GET /v1/chainhooks");
    match predicates_database_access.inner().get_all_predicates(ctx) {
        Ok(predicates) => {
            let serialized_predicates = predicates
                .iter()
                .map(|(p, s)| serialized_predicate_with_status(p, s))
                .collect::<Vec<JsonValue>>();
            return success_response(serialized_predicates.into());
        }
        Err(e) => {
            return error_response(format!("unable to retrieve predicates: {e}"), ctx);
        }
    };
}

#[openapi(tag = "Managing Predicates")]
#[post("/v1/chainhooks", format = "application/json", data = "<predicate>")]
fn handle_create_predicate(
    predicate: Result<Json<ChainhookSpecificationNetworkMap>, rocket::serde::json::Error>,
    predicates_database_access: &State<RedisPredicatesDatabaseAccess>,
    background_job_tx: &State<Arc<Mutex<Sender<ObserverCommand>>>>,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_info!(ctx, "Handling HTTP POST /v1/chainhooks");
    let predicate = match predicate {
        Err(e) => {
            return user_error_response(e.to_string(), ctx);
        }
        Ok(predicate) => {
            let predicate = predicate.into_inner();
            if let Err(e) = predicate.validate() {
                return user_error_response(e.to_string(), ctx);
            }
            predicate
        }
    };
    let predicate_uuid = predicate.get_uuid().to_string();
    if let Ok(Some(_)) = predicates_database_access
        .inner()
        .get_predicate(&predicate_uuid, ctx)
    {
        return user_error_response("Predicate uuid already in use".to_string(), ctx);
    }

    let background_job_tx = background_job_tx.inner();
    if let Ok(tx) = background_job_tx.lock() {
        let _ = tx.send(ObserverCommand::RegisterPredicate(predicate));
    };
    success_response(predicate_uuid.into())
}

#[openapi(tag = "Managing Predicates")]
#[get("/v1/chainhooks/<predicate_uuid>", format = "application/json")]
fn handle_get_predicate(
    predicate_uuid: String,
    predicates_database_access: &State<RedisPredicatesDatabaseAccess>,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_info!(ctx, "Handling HTTP GET /v1/chainhooks/{predicate_uuid}");
    let (predicate, status) = match predicates_database_access
        .inner()
        .get_predicate(&predicate_uuid, ctx)
    {
        Ok(Some(predicate_with_status)) => predicate_with_status,
        _ => return not_found_response(),
    };
    success_response(serialized_predicate_with_status(&predicate, &status))
}

#[openapi(tag = "Managing Predicates")]
#[delete("/v1/chainhooks/stacks/<predicate_uuid>", format = "application/json")]
fn handle_delete_stacks_predicate(
    predicate_uuid: String,
    background_job_tx: &State<Arc<Mutex<Sender<ObserverCommand>>>>,
    predicates_database_access: &State<RedisPredicatesDatabaseAccess>,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_info!(
        ctx,
        "Handling HTTP DELETE /v1/chainhooks/stacks/{predicate_uuid}"
    );
    if let Ok(Some(_)) = predicates_database_access
        .inner()
        .get_predicate(&predicate_uuid, ctx)
    {
        return not_found_response();
    }
    let background_job_tx = background_job_tx.inner();
    if let Ok(tx) = background_job_tx.lock() {
        let _ = tx.send(ObserverCommand::DeregisterStacksPredicate(predicate_uuid));
    };
    success_response("Ok".into())
}

#[openapi(tag = "Managing Predicates")]
#[delete("/v1/chainhooks/bitcoin/<predicate_uuid>", format = "application/json")]
fn handle_delete_bitcoin_predicate(
    predicate_uuid: String,
    predicates_database_access: &State<RedisPredicatesDatabaseAccess>,
    background_job_tx: &State<Arc<Mutex<Sender<ObserverCommand>>>>,
    ctx: &State<Context>,
) -> Result<Json<JsonValue>, Custom<Json<JsonValue>>> {
    try_info!(
        ctx,
        "Handling HTTP DELETE /v1/chainhooks/bitcoin/{predicate_uuid}"
    );
    if let Ok(Some(_)) = predicates_database_access
        .inner()
        .get_predicate(&predicate_uuid, ctx)
    {
        return not_found_response();
    }
    let background_job_tx = background_job_tx.inner();
    if let Ok(tx) = background_job_tx.lock() {
        let _ = tx.send(ObserverCommand::DeregisterBitcoinPredicate(predicate_uuid));
    };
    success_response("Ok".into())
}

pub fn document_predicate_api_server() -> Result<String, String> {
    let (_, spec) = get_routes_spec();
    let json_spec = serde_json::to_string_pretty(&spec)
        .map_err(|e| format!("failed to serialize openapi spec: {}", e))?;
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
