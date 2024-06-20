use chainhook_types::BitcoinNetwork;

use crate::chainhooks::types::ChainhookConfig;

#[cfg(feature = "stacks")]
use chainhook_types::{BitcoinBlockSignaling, StacksNetwork, StacksNodeConfig};

use super::ChainhookStore;

pub const DEFAULT_INGESTION_PORT: u16 = 20445;

#[derive(Debug, Clone)]
pub struct EventObserverConfig {
    pub chainhook_config: Option<ChainhookConfig>,
    pub bitcoind_rpc_username: String,
    pub bitcoind_rpc_password: String,
    pub bitcoind_rpc_url: String,
    pub bitcoin_network: BitcoinNetwork,
    pub prometheus_monitoring_port: Option<u16>,
    #[cfg(feature = "stacks")]
    pub bitcoin_rpc_proxy_enabled: bool,
    #[cfg(feature = "stacks")]
    pub stacks_network: StacksNetwork,
    #[cfg(feature = "stacks")]
    pub bitcoin_block_signaling: BitcoinBlockSignaling,
    #[cfg(feature = "stacks")]
    pub display_stacks_ingestion_logs: bool,
    #[cfg(not(feature = "stacks"))]
    pub zmq_url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EventObserverConfigOverrides {
    pub bitcoind_rpc_username: Option<String>,
    pub bitcoind_rpc_password: Option<String>,
    pub bitcoind_rpc_url: Option<String>,
    pub bitcoind_zmq_url: Option<String>,
    pub bitcoin_network: Option<String>,
    #[cfg(feature = "stacks")]
    pub ingestion_port: Option<u16>,
    #[cfg(feature = "stacks")]
    pub stacks_node_rpc_url: Option<String>,
    #[cfg(feature = "stacks")]
    pub display_stacks_ingestion_logs: Option<bool>,
    #[cfg(feature = "stacks")]
    pub stacks_network: Option<String>,
}

impl EventObserverConfig {
    pub fn get_bitcoin_config(&self) -> BitcoinConfig {
        #[cfg(feature = "stacks")]
        let bitcoin_block_signaling = self.bitcoin_block_signaling.clone();
        #[cfg(not(feature = "stacks"))]
        let bitcoin_block_signaling = BitcoinBlockSignaling::ZeroMQ(self.zmq_url.clone());

        let bitcoin_config = BitcoinConfig {
            username: self.bitcoind_rpc_username.clone(),
            password: self.bitcoind_rpc_password.clone(),
            rpc_url: self.bitcoind_rpc_url.clone(),
            network: self.bitcoin_network.clone(),
            bitcoin_block_signaling,
        };
        bitcoin_config
    }

    pub fn get_chainhook_store(&self) -> ChainhookStore {
        let mut chainhook_store = ChainhookStore::new();
        // If authorization not required, we create a default ChainhookConfig
        if let Some(ref chainhook_config) = self.chainhook_config {
            let mut chainhook_config = chainhook_config.clone();
            #[cfg(feature = "stacks")]
            chainhook_store
                .predicates
                .stacks_chainhooks
                .append(&mut chainhook_config.stacks_chainhooks);
            chainhook_store
                .predicates
                .bitcoin_chainhooks
                .append(&mut chainhook_config.bitcoin_chainhooks);
        }
        chainhook_store
    }

    #[cfg(feature = "stacks")]
    pub fn get_stacks_node_config(&self) -> &StacksNodeConfig {
        match self.bitcoin_block_signaling {
            BitcoinBlockSignaling::Stacks(ref config) => config,
            _ => unreachable!(),
        }
    }

    /// Helper to allow overriding some default fields in creating a new EventObserverConfig.
    ///
    // *Note: This is used by external crates, so it should not be removed, even if not used internally by Chainhook.*
    pub fn new_using_overrides(
        overrides: Option<&EventObserverConfigOverrides>,
    ) -> Result<EventObserverConfig, String> {
        let bitcoin_network =
            if let Some(network) = overrides.and_then(|c| c.bitcoin_network.as_ref()) {
                BitcoinNetwork::from_str(network)?
            } else {
                BitcoinNetwork::Regtest
            };

        #[cfg(feature = "stacks")]
        let stacks_network =
            if let Some(network) = overrides.and_then(|c| c.stacks_network.as_ref()) {
                StacksNetwork::from_str(network)?
            } else {
                StacksNetwork::Devnet
            };

        let config = EventObserverConfig {
            chainhook_config: None,
            bitcoind_rpc_username: overrides
                .and_then(|c| c.bitcoind_rpc_username.clone())
                .unwrap_or("devnet".to_string()),
            bitcoind_rpc_password: overrides
                .and_then(|c| c.bitcoind_rpc_password.clone())
                .unwrap_or("devnet".to_string()),
            bitcoind_rpc_url: overrides
                .and_then(|c| c.bitcoind_rpc_url.clone())
                .unwrap_or("http://localhost:18443".to_string()),
            bitcoin_network,
            #[cfg(feature = "stacks")]
            bitcoin_block_signaling: overrides
                .and_then(|c| c.bitcoind_zmq_url.as_ref())
                .map(|url| BitcoinBlockSignaling::ZeroMQ(url.clone()))
                .unwrap_or(BitcoinBlockSignaling::Stacks(
                    StacksNodeConfig::default_localhost(
                        overrides
                            .and_then(|c| c.ingestion_port)
                            .unwrap_or(DEFAULT_INGESTION_PORT),
                    ),
                )),
            #[cfg(feature = "stacks")]
            bitcoin_rpc_proxy_enabled: false,
            #[cfg(feature = "stacks")]
            display_stacks_ingestion_logs: overrides
                .and_then(|c| c.display_stacks_ingestion_logs)
                .unwrap_or(false),
            #[cfg(feature = "stacks")]
            stacks_network,
            #[cfg(not(feature = "stacks"))]
            zmq_url: overrides
                .and_then(|c| c.bitcoind_zmq_url.as_ref())
                .unwrap_or(&"tcp://0.0.0.0:18543".to_string())
                .to_string(),
            prometheus_monitoring_port: None,
        };
        Ok(config)
    }
}

#[derive(Debug, Clone)]
pub struct BitcoinConfig {
    pub username: String,
    pub password: String,
    pub rpc_url: String,
    pub network: BitcoinNetwork,
    pub bitcoin_block_signaling: BitcoinBlockSignaling,
}
