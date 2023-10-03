use std::{
    fs::{self, File},
    io::Read,
    net::{IpAddr, Ipv4Addr},
    thread::sleep,
    time::Duration,
};

use chainhook_sdk::utils::Context;

use crate::{
    archive::{
        default_tsv_file_path, default_tsv_sha_file_path, download_stacks_dataset_if_required,
    },
    config::{Config, EventSourceConfig, UrlConfig},
    service::tests::helpers::get_free_port,
};
use rocket::Config as RocketConfig;

const GZ_DIR: &str = "src/archive/tests/fixtures/stacks_blocks.tsv.gz";
const TMP_DIR: &str = "src/archive/tests/fixtures/tmp";
const SHA256_HASH: &str = "49ca5f80b2a1303e7f7e98a4f9d39efeb35fd9f3696c4cd9615e0b5cd1f3dcfb";

#[get("/stacks_blocks.tsv.sha256")]
fn get_sha256() -> String {
    format!("{SHA256_HASH}")
}

#[get("/stacks_blocks.tsv.gz")]
fn get_gz() -> Vec<u8> {
    let dir = format!("{}/{GZ_DIR}", env!("CARGO_MANIFEST_DIR"));
    let mut f = File::open(dir).unwrap();
    let mut buffer: Vec<u8> = Vec::new();
    f.read_to_end(&mut buffer).unwrap();
    buffer
}

async fn start_service(port: u16) {
    let config = RocketConfig::figment()
        .merge(("port", port))
        .merge(("address", IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))))
        .merge(("log_level", "off"));
    let _rocket = rocket::build()
        .configure(config)
        .mount("/", routes![get_sha256, get_gz])
        .launch()
        .await
        .unwrap();
}

#[tokio::test]
async fn it_downloads_stacks_dataset_if_required() {
    let port = get_free_port().unwrap();
    let mut config = Config::default(false, true, false, &None).unwrap();

    config.storage.working_dir = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), TMP_DIR);
    config.event_sources = vec![EventSourceConfig::StacksTsvUrl(UrlConfig {
        file_url: format!("http://0.0.0.0:{port}/stacks_blocks.tsv"),
    })];
    let _ = hiro_system_kit::thread_named("Start tsv service")
        .spawn(move || {
            let future = start_service(port);
            let _ = hiro_system_kit::nestable_block_on(future);
        })
        .expect("unable to spawn thread");

    sleep(Duration::new(1, 0));
    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };
    let mut config_clone = config.clone();
    assert!(download_stacks_dataset_if_required(&mut config, &ctx).await);
    assert!(!download_stacks_dataset_if_required(&mut config_clone, &ctx).await);

    let mut tsv_file_path = config.expected_cache_path();
    tsv_file_path.push(default_tsv_file_path(&config.network.stacks_network));
    fs::remove_file(tsv_file_path).unwrap();
    let mut tsv_sha_file_path = config.expected_cache_path();
    tsv_sha_file_path.push(default_tsv_sha_file_path(&config.network.stacks_network));
    fs::remove_file(tsv_sha_file_path).unwrap();
}
