use crate::indexer::{Indexer, IndexerConfig};

use super::config::EventObserverConfig;
use super::{start_observer_commands_handler, ObserverCommand, ObserverEvent, ObserverSidecar};

use hiro_system_kit;
use rocket::config::{self, LogLevel};
use rocket::data::{Limits, ToByteUnit};
use rocket::Config;
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};

use crate::monitoring::{start_serving_prometheus_metrics, PrometheusMonitoring};
use crate::utils::Context;

use chainhook_types::{BitcoinNetwork, StacksBlockData, StacksNetwork};

mod http;

pub async fn start_stacks_event_observer(
    config: EventObserverConfig,
    observer_commands_tx: Sender<ObserverCommand>,
    observer_commands_rx: Receiver<ObserverCommand>,
    observer_events_tx: Option<crossbeam_channel::Sender<ObserverEvent>>,
    observer_sidecar: Option<ObserverSidecar>,
    stacks_startup_context: StacksObserverStartupContext,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    let indexer_config = IndexerConfig {
        bitcoind_rpc_url: config.bitcoind_rpc_url.clone(),
        bitcoind_rpc_username: config.bitcoind_rpc_username.clone(),
        bitcoind_rpc_password: config.bitcoind_rpc_password.clone(),
        stacks_network: StacksNetwork::Devnet,
        bitcoin_network: BitcoinNetwork::Regtest,
        bitcoin_block_signaling: config.bitcoin_block_signaling.clone(),
    };

    let mut indexer = Indexer::new(indexer_config.clone());

    indexer.seed_stacks_block_pool(stacks_startup_context.block_pool_seed, &ctx);

    let log_level = if config.display_stacks_ingestion_logs {
        if cfg!(feature = "cli") {
            LogLevel::Critical
        } else {
            LogLevel::Debug
        }
    } else {
        LogLevel::Off
    };

    let ingestion_port = config.get_stacks_node_config().ingestion_port;
    let bitcoin_rpc_proxy_enabled = config.bitcoin_rpc_proxy_enabled;
    let bitcoin_config = config.get_bitcoin_config();

    let chainhook_store = config.get_chainhook_store();

    let indexer_rw_lock = Arc::new(RwLock::new(indexer));

    let background_job_tx_mutex = Arc::new(Mutex::new(observer_commands_tx.clone()));

    let prometheus_monitoring = PrometheusMonitoring::new();
    prometheus_monitoring.initialize(
        chainhook_store.predicates.stacks_chainhooks.len() as u64,
        chainhook_store.predicates.bitcoin_chainhooks.len() as u64,
        Some(stacks_startup_context.last_block_height_appended),
    );

    if let Some(port) = config.prometheus_monitoring_port {
        let registry_moved = prometheus_monitoring.registry.clone();
        let ctx_cloned = ctx.clone();
        let _ = std::thread::spawn(move || {
            let _ = hiro_system_kit::nestable_block_on(start_serving_prometheus_metrics(
                port,
                registry_moved,
                ctx_cloned,
            ));
        });
    }

    let limits = Limits::default().limit("json", 20.megabytes());
    let mut shutdown_config = config::Shutdown::default();
    shutdown_config.ctrlc = false;
    shutdown_config.grace = 0;
    shutdown_config.mercy = 0;

    let ingestion_config = Config {
        port: ingestion_port,
        workers: 1,
        address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        keep_alive: 5,
        temp_dir: std::env::temp_dir().into(),
        log_level: log_level.clone(),
        cli_colors: false,
        limits,
        shutdown: shutdown_config,
        ..Config::default()
    };

    let mut routes = rocket::routes![
        http::handle_ping,
        http::handle_new_bitcoin_block,
        http::handle_new_stacks_block,
        http::handle_new_microblocks,
        http::handle_new_mempool_tx,
        http::handle_drop_mempool_tx,
        http::handle_new_attachement,
        http::handle_mined_block,
        http::handle_mined_microblock,
    ];

    if bitcoin_rpc_proxy_enabled {
        routes.append(&mut routes![http::handle_bitcoin_rpc_call]);
        routes.append(&mut routes![http::handle_bitcoin_wallet_rpc_call]);
    }

    let ctx_cloned = ctx.clone();
    let ignite = rocket::custom(ingestion_config)
        .manage(indexer_rw_lock)
        .manage(background_job_tx_mutex)
        .manage(bitcoin_config)
        .manage(ctx_cloned)
        .manage(prometheus_monitoring.clone())
        .mount("/", routes)
        .ignite()
        .await?;
    let ingestion_shutdown = Some(ignite.shutdown());

    let _ = std::thread::spawn(move || {
        let _ = hiro_system_kit::nestable_block_on(ignite.launch());
    });

    // This loop is used for handling background jobs, emitted by HTTP calls.
    start_observer_commands_handler(
        config,
        chainhook_store,
        observer_commands_rx,
        observer_events_tx,
        ingestion_shutdown,
        prometheus_monitoring,
        observer_sidecar,
        ctx,
    )
    .await
}

#[derive(Clone, Debug, PartialEq)]
pub enum StacksChainMempoolEvent {
    TransactionsAdmitted(Vec<MempoolAdmissionData>),
    TransactionDropped(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MempoolAdmissionData {
    pub tx_data: String,
    pub tx_description: String,
}
#[derive(Debug, Clone, Default)]
pub struct StacksObserverStartupContext {
    pub block_pool_seed: Vec<StacksBlockData>,
    pub last_block_height_appended: u64,
}
