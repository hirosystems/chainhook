use crate::utils::Context;

use hiro_system_kit::slog;
use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use prometheus::{
    self,
    core::{AtomicU64, GenericGauge},
    Encoder, IntGauge, Registry, TextEncoder,
};
use rocket::serde::json::{json, Value as JsonValue};
use std::time::{SystemTime, UNIX_EPOCH};

type UInt64Gauge = GenericGauge<AtomicU64>;

#[derive(Debug, Clone)]
pub struct PrometheusMonitoring {
    pub stx_highest_block_ingested: UInt64Gauge,
    pub stx_last_reorg_timestamp: IntGauge,
    pub stx_last_reorg_applied_blocks: UInt64Gauge,
    pub stx_last_reorg_rolled_back_blocks: UInt64Gauge,
    pub stx_last_block_ingestion_time: UInt64Gauge,
    pub stx_registered_predicates: UInt64Gauge,
    pub stx_deregistered_predicates: UInt64Gauge,
    pub btc_highest_block_ingested: UInt64Gauge,
    pub btc_last_reorg_timestamp: IntGauge,
    pub btc_last_reorg_applied_blocks: UInt64Gauge,
    pub btc_last_reorg_rolled_back_blocks: UInt64Gauge,
    pub btc_last_block_ingestion_time: UInt64Gauge,
    pub btc_registered_predicates: UInt64Gauge,
    pub btc_deregistered_predicates: UInt64Gauge,
    pub registry: Registry,
}

impl PrometheusMonitoring {
    pub fn new() -> PrometheusMonitoring {
        let registry = Registry::new();
        let stx_highest_block_ingested = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "stx_highest_block_ingested",
            "The highest Stacks block ingested by the Chainhook node.",
        );
        let stx_last_reorg_timestamp = PrometheusMonitoring::create_and_register_int_gauge(
            &registry,
            "stx_last_reorg_timestamp",
            "The timestamp of the latest Stacks reorg ingested by the Chainhook node.",
        );
        let stx_last_reorg_applied_blocks = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "stx_last_reorg_applied_blocks",
            "The number of blocks applied to the Stacks chain as part of the latest Stacks reorg.",
        );
        let stx_last_reorg_rolled_back_blocks =
            PrometheusMonitoring::create_and_register_uint64_gauge(
                &registry,
                "stx_last_reorg_rolled_back_blocks",
                "The number of blocks rolled back from the Stacks chain as part of the latest Stacks reorg.",
            );
        let stx_last_block_ingestion_time = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "stx_last_block_ingestion_time",
            "The time that the Chainhook node last ingested a Stacks block.",
        );
        let stx_registered_predicates = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "stx_registered_predicates",
            "The number of Stacks predicates that have been registered by the Chainhook node.",
        );
        let stx_deregistered_predicates = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "stx_deregistered_predicates",
            "The number of Stacks predicates that have been deregistered by the Chainhook node.",
        );
        let btc_highest_block_ingested = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "btc_highest_block_ingested",
            "The highest Bitcoin block ingested by the Chainhook node.",
        );
        let btc_last_reorg_timestamp = PrometheusMonitoring::create_and_register_int_gauge(
            &registry,
            "btc_last_reorg_timestamp",
            "The timestamp of the latest Bitcoin reorg ingested by the Chainhook node.",
        );
        let btc_last_reorg_applied_blocks =
            PrometheusMonitoring::create_and_register_uint64_gauge(
                &registry,
                "btc_last_reorg_applied_blocks",
                "The number of blocks applied to the Bitcoin chain as part of the latest Bitcoin reorg.",
            );
        let btc_last_reorg_rolled_back_blocks =
            PrometheusMonitoring::create_and_register_uint64_gauge(
                &registry,
                "btc_last_reorg_rolled_back_blocks",
                "The number of blocks rolled back from the Bitcoin chain as part of the latest Bitcoin reorg.",
            );
        let btc_last_block_ingestion_time = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "btc_last_block_ingestion_time",
            "The time that the Chainhook node last ingested a Bitcoin block.",
        );
        let btc_registered_predicates = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "btc_registered_predicates",
            "The number of Bitcoin predicates that have been registered by the Chainhook node.",
        );
        let btc_deregistered_predicates = PrometheusMonitoring::create_and_register_uint64_gauge(
            &registry,
            "btc_deregistered_predicates",
            "The number of Bitcoin predicates that have been deregistered by the Chainhook node.",
        );

        PrometheusMonitoring {
            stx_highest_block_ingested,
            stx_last_reorg_timestamp,
            stx_last_reorg_applied_blocks,
            stx_last_reorg_rolled_back_blocks,
            stx_last_block_ingestion_time,
            stx_registered_predicates,
            stx_deregistered_predicates,
            btc_highest_block_ingested,
            btc_last_reorg_timestamp,
            btc_last_reorg_applied_blocks,
            btc_last_reorg_rolled_back_blocks,
            btc_last_block_ingestion_time,
            btc_registered_predicates,
            btc_deregistered_predicates,
            registry,
        }
    }

    pub fn create_and_register_uint64_gauge(
        registry: &Registry,
        name: &str,
        help: &str,
    ) -> UInt64Gauge {
        let g = UInt64Gauge::new(name, help).unwrap();
        registry.register(Box::new(g.clone())).unwrap();
        g
    }

    pub fn create_and_register_int_gauge(registry: &Registry, name: &str, help: &str) -> IntGauge {
        let g = IntGauge::new(name, help).unwrap();
        registry.register(Box::new(g.clone())).unwrap();
        g
    }

    pub fn stx_metrics_deregister_predicate(&self) {
        self.stx_registered_predicates.dec();
        self.stx_deregistered_predicates.inc();
    }

    pub fn stx_metrics_register_predicate(&self) {
        self.stx_registered_predicates.inc();
    }
    pub fn stx_metrics_set_registered_predicates(&self, registered_predicates: u64) {
        self.stx_registered_predicates.set(registered_predicates);
    }

    pub fn stx_metrics_set_reorg(
        &self,
        timestamp: i64,
        applied_blocks: u64,
        rolled_back_blocks: u64,
    ) {
        self.stx_last_reorg_timestamp.set(timestamp);
        self.stx_last_reorg_applied_blocks.set(applied_blocks);
        self.stx_last_reorg_rolled_back_blocks
            .set(rolled_back_blocks);
    }

    pub fn stx_metrics_ingest_block(&self, new_block_height: u64) {
        let highest_ingested = self.stx_highest_block_ingested.get();
        if new_block_height > highest_ingested {
            self.stx_highest_block_ingested.set(new_block_height);
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Could not get current time in ms")
            .as_secs() as u64;
        self.stx_last_block_ingestion_time.set(time);
    }

    pub fn btc_metrics_deregister_predicate(&self) {
        self.btc_registered_predicates.dec();
        self.btc_deregistered_predicates.inc();
    }

    pub fn btc_metrics_register_predicate(&self) {
        self.btc_registered_predicates.inc();
    }

    pub fn btc_metrics_set_registered_predicates(&self, registered_predicates: u64) {
        self.btc_registered_predicates.set(registered_predicates);
    }

    pub fn btc_metrics_set_reorg(
        &self,
        timestamp: i64,
        applied_blocks: u64,
        rolled_back_blocks: u64,
    ) {
        self.btc_last_reorg_timestamp.set(timestamp);
        self.btc_last_reorg_applied_blocks.set(applied_blocks);
        self.btc_last_reorg_rolled_back_blocks
            .set(rolled_back_blocks);
    }

    pub fn btc_metrics_ingest_block(&self, new_block_height: u64) {
        let highest_ingested = self.btc_highest_block_ingested.get();
        if new_block_height > highest_ingested {
            self.btc_highest_block_ingested.set(new_block_height);
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Could not get current time in ms")
            .as_secs() as u64;
        self.btc_last_block_ingestion_time.set(time);
    }

    pub fn get_metrics(&self) -> JsonValue {
        json!({
            "bitcoin": {
                "tip_height": self.btc_highest_block_ingested.get(),
                "last_block_ingestion_at": self.btc_last_block_ingestion_time.get(),
                "last_reorg": {
                    "timestamp": self.btc_last_reorg_timestamp.get(),
                    "applied_blocks": self.btc_last_reorg_applied_blocks.get(),
                    "rolled_back_blocks": self.btc_last_reorg_rolled_back_blocks.get(),
                },
                "registered_predicates": self.btc_registered_predicates.get(),
                "deregistered_predicates": self.btc_deregistered_predicates.get(),
            },
            "stacks": {
                "tip_height": self.stx_highest_block_ingested.get(),
                "last_block_ingestion_at": self.stx_last_block_ingestion_time.get(),
                "last_reorg": {
                    "timestamp": self.stx_last_reorg_timestamp.get(),
                    "applied_blocks": self.stx_last_reorg_applied_blocks.get(),
                    "rolled_back_blocks": self.stx_last_reorg_rolled_back_blocks.get(),
                },
                "registered_predicates": self.stx_registered_predicates.get(),
                "deregistered_predicates": self.stx_deregistered_predicates.get(),
            }
        })
    }
}

async fn serve_req(
    req: Request<Body>,
    registry: Registry,
    ctx: Context,
) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            ctx.try_log(|logger| {
                slog::debug!(
                    logger,
                    "Prometheus monitoring: respsonding to metrics request"
                )
            });

            let encoder = TextEncoder::new();
            let metric_families = registry.gather();
            let mut buffer = vec![];
            let response = match encoder.encode(&metric_families, &mut buffer) {
                Ok(_) => Response::builder()
                    .status(200)
                    .header(CONTENT_TYPE, encoder.format_type())
                    .body(Body::from(buffer))
                    .unwrap(),
                Err(e) => {
                    ctx.try_log(|logger| {
                        slog::debug!(
                            logger,
                            "Prometheus monitoring: failed to encode metrics: {}",
                            e.to_string()
                        )
                    });
                    Response::builder().status(500).body(Body::empty()).unwrap()
                }
            };
            Ok(response)
        }
        (_, _) => {
            ctx.try_log(|logger| {
                slog::debug!(
                    logger,
                    "Prometheus monitoring: received request with invalid method/route: {}/{}",
                    req.method(),
                    req.uri().path()
                )
            });
            let response = Response::builder().status(404).body(Body::empty()).unwrap();

            Ok(response)
        }
    }
}

pub async fn start_serving_prometheus_metrics(port: u16, registry: Registry, ctx: Context) {
    let addr = ([127, 0, 0, 1], port).into();
    let ctx_clone = ctx.clone();
    let make_svc = make_service_fn(|_| {
        let registry = registry.clone();
        let ctx_clone = ctx_clone.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |r| {
                serve_req(r, registry.clone(), ctx_clone.clone())
            }))
        }
    });
    let serve_future = Server::bind(&addr).serve(make_svc);

    ctx.try_log(|logger| slog::info!(logger, "Prometheus monitoring: listening on port {}", port));

    if let Err(err) = serve_future.await {
        ctx.try_log(|logger| slog::warn!(logger, "Prometheus monitoring: server error: {}", err));
    }
}

#[cfg(test)]
mod test {
    use std::{thread::sleep, time::Duration};

    use super::PrometheusMonitoring;

    #[test]
    fn it_tracks_stx_predicate_registration_deregistration_with_defaults() {
        let prometheus = PrometheusMonitoring::new();
        assert_eq!(prometheus.stx_registered_predicates.get(), 0);
        assert_eq!(prometheus.stx_deregistered_predicates.get(), 0);
        prometheus.stx_metrics_set_registered_predicates(10);
        assert_eq!(prometheus.stx_registered_predicates.get(), 10);
        assert_eq!(prometheus.stx_deregistered_predicates.get(), 0);
        prometheus.stx_metrics_register_predicate();
        assert_eq!(prometheus.stx_registered_predicates.get(), 11);
        assert_eq!(prometheus.stx_deregistered_predicates.get(), 0);
        prometheus.stx_metrics_deregister_predicate();
        assert_eq!(prometheus.stx_registered_predicates.get(), 10);
        assert_eq!(prometheus.stx_deregistered_predicates.get(), 1);
    }

    #[test]
    fn it_tracks_stx_reorgs() {
        let prometheus = PrometheusMonitoring::new();
        assert_eq!(prometheus.stx_last_reorg_timestamp.get(), 0);
        assert_eq!(prometheus.stx_last_reorg_applied_blocks.get(), 0);
        assert_eq!(prometheus.stx_last_reorg_rolled_back_blocks.get(), 0);
        prometheus.stx_metrics_set_reorg(10000, 1, 1);
        assert_eq!(prometheus.stx_last_reorg_timestamp.get(), 10000);
        assert_eq!(prometheus.stx_last_reorg_applied_blocks.get(), 1);
        assert_eq!(prometheus.stx_last_reorg_rolled_back_blocks.get(), 1);
    }

    #[test]
    fn it_tracks_stx_block_ingestion() {
        let prometheus = PrometheusMonitoring::new();
        assert_eq!(prometheus.stx_highest_block_ingested.get(), 0);
        assert_eq!(prometheus.stx_last_block_ingestion_time.get(), 0);
        prometheus.stx_metrics_ingest_block(100);
        assert_eq!(prometheus.stx_highest_block_ingested.get(), 100);
        let time = prometheus.stx_last_block_ingestion_time.get();
        assert!(time > 0);
        // ingesting a block lower than previous tip will
        // update ingestion time but not highest block ingested
        sleep(Duration::new(1, 0));
        prometheus.stx_metrics_ingest_block(99);
        assert_eq!(prometheus.stx_highest_block_ingested.get(), 100);
        assert!(prometheus.stx_last_block_ingestion_time.get() > time);
    }

    #[test]
    fn it_tracks_btc_predicate_registration_deregistration_with_defaults() {
        let prometheus = PrometheusMonitoring::new();
        assert_eq!(prometheus.btc_registered_predicates.get(), 0);
        assert_eq!(prometheus.btc_deregistered_predicates.get(), 0);
        prometheus.btc_metrics_set_registered_predicates(10);
        assert_eq!(prometheus.btc_registered_predicates.get(), 10);
        assert_eq!(prometheus.btc_deregistered_predicates.get(), 0);
        prometheus.btc_metrics_register_predicate();
        assert_eq!(prometheus.btc_registered_predicates.get(), 11);
        assert_eq!(prometheus.btc_deregistered_predicates.get(), 0);
        prometheus.btc_metrics_deregister_predicate();
        assert_eq!(prometheus.btc_registered_predicates.get(), 10);
        assert_eq!(prometheus.btc_deregistered_predicates.get(), 1);
    }

    #[test]
    fn it_tracks_btc_reorgs() {
        let prometheus = PrometheusMonitoring::new();
        assert_eq!(prometheus.btc_last_reorg_timestamp.get(), 0);
        assert_eq!(prometheus.btc_last_reorg_applied_blocks.get(), 0);
        assert_eq!(prometheus.btc_last_reorg_rolled_back_blocks.get(), 0);
        prometheus.btc_metrics_set_reorg(10000, 1, 1);
        assert_eq!(prometheus.btc_last_reorg_timestamp.get(), 10000);
        assert_eq!(prometheus.btc_last_reorg_applied_blocks.get(), 1);
        assert_eq!(prometheus.btc_last_reorg_rolled_back_blocks.get(), 1);
    }

    #[test]
    fn it_tracks_btc_block_ingestion() {
        let prometheus = PrometheusMonitoring::new();
        assert_eq!(prometheus.btc_highest_block_ingested.get(), 0);
        assert_eq!(prometheus.btc_last_block_ingestion_time.get(), 0);
        prometheus.btc_metrics_ingest_block(100);
        assert_eq!(prometheus.btc_highest_block_ingested.get(), 100);
        let time = prometheus.btc_last_block_ingestion_time.get();
        assert!(time > 0);
        // ingesting a block lower than previous tip will
        // update ingestion time but not highest block ingested
        sleep(Duration::new(1, 0));
        prometheus.btc_metrics_ingest_block(99);
        assert_eq!(prometheus.btc_highest_block_ingested.get(), 100);
        assert!(prometheus.btc_last_block_ingestion_time.get() > time);
    }
}
