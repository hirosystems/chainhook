use std::cmp::max;
use std::collections::HashMap;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::RwLock;

use crate::scan::stacks::Record;
use crate::scan::stacks::RecordKind;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::hashes::sha256d::Hash;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::Amount;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::BlockHash;
use chainhook_sdk::bitcoincore_rpc_json::bitcoin::TxMerkleNode;
use chainhook_sdk::bitcoincore_rpc_json::GetBlockResult;
use chainhook_sdk::bitcoincore_rpc_json::GetBlockchainInfoResult;
use chainhook_sdk::bitcoincore_rpc_json::GetNetworkInfoResult;
use chainhook_sdk::indexer::stacks::NewBlock;
use chainhook_sdk::indexer::stacks::NewTransaction;
use rocket::serde::json::Json;
use rocket::Config;
use rocket::State;
use serde_json::Value;

fn create_stacks_new_transaction(index: u64) -> NewTransaction {
    NewTransaction {
        txid: format!("transaction_id_{index}"),
        tx_index: index as usize,
        status: format!("success"),
        raw_result: format!("0x0703"),
        raw_tx: format!("0x00000000010400e2cd0871da5bdd38c4d5569493dc3b14aac4e0a10000000000000019000000000000000000008373b16e4a6f9d87864c314dd77bbd8b27a2b1805e96ec5a6509e7e4f833cd6a7bdb2462c95f6968a867ab6b0e8f0a6498e600dbc46cfe9f84c79709da7b9637010200000000040000000000000000000000000000000000000000000000000000000000000000"),
        execution_cost: None,
    }
}

pub fn create_stacks_new_block(height: u64, burn_block_height: u64) -> NewBlock {
    let parent_height = if height == 0 { 0 } else { height - 1 };
    let parent_burn_block_height = if burn_block_height == 0 {
        0
    } else {
        burn_block_height - 1
    };

    NewBlock {
        block_height: height,
        block_hash: height_to_prefixed_hash(height),
        index_block_hash: height_to_prefixed_hash(height),
        burn_block_height: burn_block_height,
        burn_block_hash: height_to_prefixed_hash(burn_block_height),
        parent_block_hash: height_to_prefixed_hash(parent_height),
        parent_index_block_hash: height_to_prefixed_hash(parent_height),
        parent_microblock: "0x0000000000000000000000000000000000000000000000000000000000000000"
            .into(),
        parent_microblock_sequence: 0,
        parent_burn_block_hash: height_to_prefixed_hash(parent_burn_block_height),
        parent_burn_block_height: burn_block_height,
        parent_burn_block_timestamp: 0,
        transactions: (0..4).map(|i| create_stacks_new_transaction(i)).collect(),
        events: vec![],
        matured_miner_rewards: vec![],
    }
}

fn create_stacks_block_received_record(height: u64, burn_block_height: u64) -> Record {
    let block = create_stacks_new_block(height, burn_block_height);
    let serialized_block = serde_json::to_string(&block).unwrap();
    Record {
        id: height,
        created_at: height.to_string(),
        kind: RecordKind::StacksBlockReceived,
        blob: Some(serialized_block),
    }
}
pub const WORKING_DIR: &str = "src/service/tests/fixtures/tmp";
pub fn write_stacks_blocks_to_tsv(block_count: u64, dir: &str) {
    let mut writer = csv::WriterBuilder::default()
        .has_headers(false)
        .delimiter(b'\t')
        .double_quote(false)
        .quote(b'\'')
        .buffer_capacity(8 * (1 << 10))
        .from_path(dir)
        .expect("unable to create csv writer");
    for i in 1..block_count + 1 {
        writer
            .serialize(create_stacks_block_received_record(i, i + 100))
            .unwrap();
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(crate = "rocket::serde")]
struct Rpc {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Vec<Value>,
}

pub fn height_to_prefixed_hash(height: u64) -> String {
    format!("0x{}", height_to_hash_str(height))
}
fn height_to_hash_str(height: u64) -> String {
    format!("{:0>64}", height.to_string())
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
    let config = Config {
        port,
        address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        ..Config::debug_default()
    };
    let chain_tip_rw_lock = Arc::new(RwLock::new(starting_chain_tip));
    let _rocket = rocket::build()
        .configure(config)
        .manage(chain_tip_rw_lock)
        .mount("/", routes![handle_rpc, handle_increment_chain_tip])
        .launch()
        .await
        .unwrap();
}
