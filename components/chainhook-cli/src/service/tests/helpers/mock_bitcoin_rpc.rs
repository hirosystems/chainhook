use chainhook_sdk::bitcoincore_rpc_json::bitcoin::TxMerkleNode;
use rocket::serde::json::Value;
use std::cmp::max;
use std::collections::HashMap;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::RwLock;

use chainhook_sdk::bitcoincore_rpc_json::bitcoin::hashes::sha256d::Hash;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::Amount;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::BlockHash;
use chainhook_sdk::bitcoincore_rpc_json::GetBlockResult;
use chainhook_sdk::bitcoincore_rpc_json::GetBlockchainInfoResult;
use chainhook_sdk::bitcoincore_rpc_json::GetNetworkInfoResult;
use rocket::serde::json::Json;
use rocket::Config;
use rocket::State;

use super::height_to_hash_str;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(crate = "rocket::serde")]
struct Rpc {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Vec<Value>,
}

fn height_to_hash(height: u64) -> BlockHash {
    let hash = Hash::from_str(&height_to_hash_str(height)).unwrap();
    BlockHash::from_hash(hash)
}

fn height_to_merkle_node(height: u64) -> TxMerkleNode {
    let hash = Hash::from_str(&height_to_hash_str(height)).unwrap();
    TxMerkleNode::from_hash(hash)
}

#[post("/increment-chain-tip")]
fn handle_increment_chain_tip(chain_tip: &State<Arc<RwLock<u64>>>) -> Value {
    let mut chain_tip = chain_tip.inner().write().unwrap();
    *chain_tip += 1;
    json!(chain_tip.to_owned())
}

#[post("/", format = "application/json", data = "<rpc>")]
fn handle_rpc(rpc: Json<Rpc>, chain_tip: &State<Arc<RwLock<u64>>>) -> Value {
    let rpc = rpc.into_inner();
    let chain_tip = *chain_tip.inner().read().unwrap();
    match rpc.method.as_str() {
        "getblock" => {
            let hash = rpc.params[0].as_str().unwrap();
            let prefix = hash.chars().take_while(|&ch| ch == '0').collect::<String>();
            let height = hash.split(&prefix).collect::<Vec<&str>>()[1];
            let height = height.parse::<u64>().unwrap();
            if height > chain_tip {
                return json!({
                    "id": rpc.id,
                    "jsonrpc": rpc.jsonrpc,
                    "error": format!("invalid request: requested block is above chain tip: height {}, chain tip: {}", height, chain_tip)
                });
            }
            let next_block_hash = if height == chain_tip {
                None
            } else {
                Some(height_to_hash(height + 1))
            };
            let confirmations = max(0, chain_tip - height) as i32;
            let block = GetBlockResult {
                hash: BlockHash::from_hash(Hash::from_str(hash).unwrap()),
                confirmations,
                size: 0,
                strippedsize: None,
                weight: 0,
                height: height as usize,
                version: 19000,
                version_hex: None,
                merkleroot: height_to_merkle_node(height),
                tx: vec![],
                time: 0,
                mediantime: None,
                nonce: 0,
                bits: "".to_string(),
                difficulty: 0.0,
                chainwork: vec![],
                n_tx: 0,
                previousblockhash: Some(height_to_hash(height - 1)),
                nextblockhash: next_block_hash,
            };
            json!({
                "id": rpc.id,
                "jsonrpc": rpc.jsonrpc,
                "result": serde_json::to_value(&block).unwrap()
            })
        }
        "getblockchaininfo" => {
            let hash = format!("{:0>64}", chain_tip.to_string());
            let hash = Hash::from_str(&hash).unwrap();
            let blockchain_info = GetBlockchainInfoResult {
                chain: "regtest".into(),
                blocks: chain_tip.to_owned(),
                headers: 0,
                best_block_hash: BlockHash::from_hash(hash),
                difficulty: 0.0,
                median_time: 0,
                verification_progress: 0.0,
                initial_block_download: false,
                chain_work: vec![],
                size_on_disk: 0,
                pruned: false,
                prune_height: None,
                automatic_pruning: None,
                prune_target_size: None,
                softforks: HashMap::new(),
                warnings: "".into(),
            };
            json!({
                "id": rpc.id,
                "jsonrpc": rpc.jsonrpc,
                "result": serde_json::to_value(&blockchain_info).unwrap()
            })
        }
        "getnetworkinfo" => {
            let network_info = GetNetworkInfoResult {
                version: 190000,
                subversion: "".into(),
                protocol_version: 0,
                local_services: "".into(),
                local_relay: false,
                time_offset: 0,
                connections: 0,
                connections_in: None,
                connections_out: None,
                network_active: true,
                networks: vec![],
                relay_fee: Amount::ZERO,
                incremental_fee: Amount::ZERO,
                local_addresses: vec![],
                warnings: "".into(),
            };
            let value = serde_json::to_value(network_info).unwrap();
            json!({
                "id": rpc.id,
                "jsonrpc": rpc.jsonrpc,
                "result": value
            })
        }
        "getblockhash" => {
            let height = rpc.params[0].as_u64().unwrap();
            let hash = format!("{:0>64}", height.to_string());
            let hash = Hash::from_str(&hash).unwrap();
            let hash = BlockHash::from_hash(hash);
            json!({
                "id": serde_json::to_value(rpc.id).unwrap(),
                "jsonrpc": rpc.jsonrpc,
                "result": serde_json::to_value(hash).unwrap(),
            })
        }
        _ => unimplemented!("unsupported rpc endpoint"),
    }
}

pub async fn mock_bitcoin_rpc(port: u16, starting_chain_tip: u64) {
    let config = Config::figment()
        .merge(("port", port))
        .merge(("address", IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))))
        .merge(("log_level", "off"));
    let chain_tip_rw_lock = Arc::new(RwLock::new(starting_chain_tip));
    let _rocket = rocket::build()
        .configure(config)
        .manage(chain_tip_rw_lock)
        .mount("/", routes![handle_rpc, handle_increment_chain_tip])
        .launch()
        .await
        .unwrap();
}
