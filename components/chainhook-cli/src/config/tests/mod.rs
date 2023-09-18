use std::path::PathBuf;

use crate::config::{file::NetworkConfigMode, PredicatesApi, PredicatesApiConfig};

use super::{generator::generate_config, Config, ConfigFile, EventSourceConfig, PathConfig};
use chainhook_sdk::types::{BitcoinNetwork, StacksNetwork};
use test_case::test_case;

const LOCAL_DIR: &str = env!("CARGO_MANIFEST_DIR");
#[test_case(BitcoinNetwork::Regtest)]
#[test_case(BitcoinNetwork::Testnet)]
#[test_case(BitcoinNetwork::Mainnet)]
fn config_from_file_matches_generator_for_all_networks(network: BitcoinNetwork) {
    let mode = NetworkConfigMode::from_bitcoin_network(&network);
    let path = format!(
        "{}/src/config/tests/fixtures/{}_chainhook.toml",
        LOCAL_DIR,
        mode.as_str()
    );
    let from_path_config = Config::from_file_path(&path).unwrap();
    let generated_config_str = generate_config(&network);
    let generated_config_file: ConfigFile = toml::from_str(&generated_config_str).unwrap();
    let generated_config = Config::from_config_file(generated_config_file).unwrap();
    assert_eq!(generated_config, from_path_config);
}

#[test]
fn config_from_file_allows_local_tsv_file() {
    let path = format!(
        "{}/src/config/tests/fixtures/local_tsv_chainhook.toml",
        LOCAL_DIR,
    );

    Config::from_file_path(&path).expect("failed to generate config with local tsv path");
}

#[test]
fn parse_config_from_file_rejects_config_with_unsupported_mode() {
    let path = format!(
        "{}/src/config/tests/fixtures/unsupported_chainhook.toml",
        LOCAL_DIR
    );
    Config::from_file_path(&path)
        .expect_err("Did not reject unsupported network mode as expected.");
}

#[test]
fn is_http_api_enabled_handles_both_modes() {
    let mut config = Config::default(true, false, false, &None).unwrap();
    assert!(!config.is_http_api_enabled());
    config.http_api = PredicatesApi::On(PredicatesApiConfig {
        http_port: 0,
        database_uri: format!(""),
        display_logs: false,
    });
    assert!(config.is_http_api_enabled());
}

#[test]
fn should_download_remote_stacks_tsv_handles_both_modes() {
    let url_src = EventSourceConfig::StacksTsvUrl(super::UrlConfig {
        file_url: String::new(),
    });
    let path_src = EventSourceConfig::StacksTsvPath(PathConfig {
        file_path: PathBuf::new(),
    });
    let mut config = Config::default(true, false, false, &None).unwrap();

    config.event_sources = vec![url_src.clone(), path_src.clone()];
    assert_eq!(config.should_download_remote_stacks_tsv(), false);

    config.event_sources = vec![path_src.clone()];
    assert_eq!(config.should_download_remote_stacks_tsv(), false);

    config.event_sources = vec![];
    assert_eq!(config.should_download_remote_stacks_tsv(), false);

    config.event_sources = vec![url_src.clone()];
    assert_eq!(config.should_download_remote_stacks_tsv(), true);
}

#[test]
#[should_panic(expected = "expected remote-tsv source")]
fn expected_remote_stacks_tsv_base_url_panics_if_missing() {
    let url_src = EventSourceConfig::StacksTsvUrl(super::UrlConfig {
        file_url: format!("test"),
    });
    let mut config = Config::default(true, false, false, &None).unwrap();

    config.event_sources = vec![url_src.clone()];
    assert_eq!(config.expected_remote_stacks_tsv_base_url(), "test");

    config.event_sources = vec![];
    config.expected_remote_stacks_tsv_base_url();
}

#[test]
#[should_panic(expected = "expected local-tsv source")]
fn expected_local_stacks_tsv_base_url_panics_if_missing() {
    let path = PathBuf::from("test");
    let path_src = EventSourceConfig::StacksTsvPath(PathConfig {
        file_path: path.clone(),
    });
    let mut config = Config::default(true, false, false, &None).unwrap();

    config.event_sources = vec![path_src.clone()];
    assert_eq!(config.expected_local_stacks_tsv_file(), &path);

    config.event_sources = vec![];
    config.expected_local_stacks_tsv_file();
}

#[test]
fn add_local_stacks_tsv_source_allows_adding_src() {
    let mut config = Config::default(true, false, false, &None).unwrap();
    assert_eq!(config.event_sources.len(), 0);
    let path = PathBuf::from("test");
    config.add_local_stacks_tsv_source(&path);
    assert_eq!(config.event_sources.len(), 1);
}
#[test]
fn it_has_default_config_for_each_network() {
    let config = Config::default(true, false, false, &None).unwrap();
    assert_eq!(config.network.bitcoin_network, BitcoinNetwork::Regtest);
    assert_eq!(config.network.stacks_network, StacksNetwork::Devnet);
    let config = Config::default(false, true, false, &None).unwrap();
    assert_eq!(config.network.bitcoin_network, BitcoinNetwork::Testnet);
    assert_eq!(config.network.stacks_network, StacksNetwork::Testnet);
    let config = Config::default(false, false, true, &None).unwrap();
    assert_eq!(config.network.bitcoin_network, BitcoinNetwork::Mainnet);
    assert_eq!(config.network.stacks_network, StacksNetwork::Mainnet);
    let path = format!(
        "{}/src/config/tests/fixtures/devnet_chainhook.toml",
        LOCAL_DIR
    );
    let config = Config::default(false, false, false, &Some(path)).unwrap();
    assert_eq!(config.network.bitcoin_network, BitcoinNetwork::Regtest);
    assert_eq!(config.network.stacks_network, StacksNetwork::Devnet);
    Config::default(true, true, false, &None).expect_err("expected invalid combination error");
}
