use chainhook_sdk::bitcoin::Network;
use chainhook_sdk::bitcoincore_rpc_json::GetRawTransactionResultVoutScriptPubKey;
use chainhook_sdk::indexer::bitcoin::BitcoinBlockFullBreakdown;
use chainhook_sdk::indexer::bitcoin::BitcoinTransactionFullBreakdown;
use chainhook_sdk::indexer::bitcoin::BitcoinTransactionInputFullBreakdown;
use chainhook_sdk::indexer::bitcoin::BitcoinTransactionInputPrevoutFullBreakdown;
use chainhook_sdk::indexer::bitcoin::BitcoinTransactionOutputFullBreakdown;
use chainhook_sdk::indexer::bitcoin::GetRawTransactionResultVinScriptSig;
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
use chainhook_sdk::bitcoincore_rpc_json::GetBlockchainInfoResult;
use chainhook_sdk::bitcoincore_rpc_json::GetNetworkInfoResult;
use rocket::serde::json::Json;
use rocket::Config;
use rocket::State;

use super::branch_and_height_to_hash_str;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(crate = "rocket::serde")]
struct Rpc {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Vec<Value>,
}

fn branch_and_height_to_hash(branch: Option<char>, height: u64) -> BlockHash {
    let hash = Hash::from_str(&branch_and_height_to_hash_str(branch, height)).unwrap();
    BlockHash::from_raw_hash(hash)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct TipData {
    pub branch: BranchKey,
    pub parent_branch_key: Option<BranchKey>,
    pub parent_height_at_fork: Option<Height>,
}

#[post(
    "/increment-chain-tip",
    format = "application/json",
    data = "<tip_data>"
)]
fn handle_increment_chain_tip(
    tip_data: Json<TipData>,
    fork_tracker_rw_lock: &State<Arc<RwLock<HashMap<BranchKey, ForkData>>>>,
) -> Value {
    let tip_data = tip_data.into_inner();
    let branch = tip_data.branch;
    let mut fork_tracker = fork_tracker_rw_lock.inner().write().unwrap();
    let (chain_tip, _parent_info) = match fork_tracker.get_mut(&branch) {
        None => {
            let parent_branch = tip_data.parent_branch_key.unwrap();
            let parent_height_at_fork = tip_data.parent_height_at_fork.unwrap();
            let branch_chain_tip = parent_height_at_fork + 1;
            fork_tracker.insert(
                branch,
                (
                    branch_chain_tip,
                    Some((parent_branch, parent_height_at_fork)),
                ),
            );
            return json!(branch_chain_tip);
        }
        Some(tip) => tip,
    };
    *chain_tip += 1;
    json!(chain_tip.to_owned())
}

#[post("/", format = "application/json", data = "<rpc>")]
fn handle_rpc(
    rpc: Json<Rpc>,
    fork_tracker_rw_lock: &State<Arc<RwLock<HashMap<BranchKey, ForkData>>>>,
) -> Value {
    let rpc = rpc.into_inner();
    let fork_tracker = fork_tracker_rw_lock.inner().read().unwrap();
    match rpc.method.as_str() {
        "getblock" => {
            let hash = rpc.params[0].as_str().unwrap();
            let mut chars = hash.chars();
            let branch = chars.next().unwrap();
            let prefix = chars.take_while(|&ch| ch == '0').collect::<String>();
            let height = hash.split(&prefix).collect::<Vec<&str>>()[1];
            let height = height.parse::<u64>().unwrap_or(0);
            let (chain_tip, parent_data) = fork_tracker.get(&branch).unwrap_or(&(0, None));
            if &height > chain_tip {
                return json!({
                    "id": rpc.id,
                    "jsonrpc": rpc.jsonrpc,
                    "error": format!("invalid request: requested block is above chain tip: height {}, chain tip: {}", height, chain_tip)
                });
            }

            let confirmations = max(0, chain_tip - height) as i32;

            let previousblockhash = if height == 0 {
                None
            } else {
                let parent_height = height - 1;
                let mut parent_branch = branch;
                if let Some((parent_branch_key, parent_height_at_fork)) = parent_data {
                    if &parent_height == parent_height_at_fork {
                        parent_branch = *parent_branch_key;
                    }
                }
                Some(branch_and_height_to_hash_str(
                    Some(parent_branch),
                    parent_height,
                ))
            };

            let coinbase = BitcoinTransactionFullBreakdown {
                txid: branch_and_height_to_hash_str(Some(branch), height),
                vin: vec![BitcoinTransactionInputFullBreakdown {
                    sequence: 0,
                    txid: None,
                    vout: None,
                    script_sig: None,
                    txinwitness: None,
                    prevout: None,
                }],
                vout: vec![BitcoinTransactionOutputFullBreakdown {
                    value: Amount::ZERO,
                    n: 0,
                    script_pub_key: GetRawTransactionResultVoutScriptPubKey {
                        asm: format!(""),
                        hex: vec![],
                        req_sigs: None,
                        type_: None,
                        addresses: vec![],
                        address: None,
                    },
                }],
            };
            let tx = BitcoinTransactionFullBreakdown {
                txid: branch_and_height_to_hash_str(Some(branch), height + 1),
                vin: vec![BitcoinTransactionInputFullBreakdown {
                    sequence: 0,
                    txid: Some(branch_and_height_to_hash_str(Some(branch), height + 1)),
                    vout: Some(1),
                    script_sig: Some(GetRawTransactionResultVinScriptSig { hex: format!("") }),
                    txinwitness: Some(vec![format!("")]),
                    prevout: Some(BitcoinTransactionInputPrevoutFullBreakdown {
                        height: height,
                        value: Amount::ZERO,
                    }),
                }],
                vout: vec![BitcoinTransactionOutputFullBreakdown {
                    value: Amount::ZERO,
                    n: 0,
                    script_pub_key: GetRawTransactionResultVoutScriptPubKey {
                        asm: format!(""),
                        hex: vec![],
                        req_sigs: None,
                        type_: None,
                        addresses: vec![],
                        address: None,
                    },
                }],
            };
            let block = BitcoinBlockFullBreakdown {
                hash: hash.into(),
                confirmations,
                height: height as usize,
                tx: vec![coinbase, tx],
                time: 0,
                nonce: 0,
                previousblockhash,
            };
            json!({
                "id": rpc.id,
                "jsonrpc": rpc.jsonrpc,
                "result": serde_json::to_value(&block).unwrap()
            })
        }
        "getblockchaininfo" => {
            let (branch, (chain_tip, _)) = fork_tracker
                .iter()
                .max_by(|a, b| a.1.cmp(&b.1))
                .map(|kv| kv)
                .unwrap();

            let hash = branch_and_height_to_hash(Some(*branch), *chain_tip);
            let blockchain_info = GetBlockchainInfoResult {
                chain: Network::Regtest,
                blocks: chain_tip.to_owned(),
                headers: 0,
                best_block_hash: hash,
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
            let (branch, _) = fork_tracker
                .iter()
                .max_by(|a, b| a.1.cmp(&b.1))
                .map(|kv| kv)
                .unwrap();

            let height = rpc.params[0].as_u64().unwrap();
            let hash = branch_and_height_to_hash(Some(*branch), height);
            json!({
                "id": serde_json::to_value(rpc.id).unwrap(),
                "jsonrpc": rpc.jsonrpc,
                "result": serde_json::to_value(hash).unwrap(),
            })
        }
        "gettxoutproof" => {
            json!({
                "id": serde_json::to_value(rpc.id).unwrap(),
                "jsonrpc": rpc.jsonrpc,
                "result": "00",
            })
        }
        "getaddressinfo" => {
            json!({
                "id": serde_json::to_value(rpc.id).unwrap(),
                "jsonrpc": rpc.jsonrpc,
                "result": {
                    "address": rpc.params[0]
                },
            })
        }
        "sendrawtransaction" => {
            json!({
                "id": serde_json::to_value(rpc.id).unwrap(),
                "jsonrpc": rpc.jsonrpc,
                "result": "success",
            })
        }
        _ => unimplemented!("unsupported rpc endpoint: {}", rpc.method.as_str()),
    }
}

type BranchKey = char;
type Height = u64;
type ForkPoint = (BranchKey, Height);
type ForkData = (Height, Option<ForkPoint>);
pub async fn mock_bitcoin_rpc(port: u16, starting_chain_tip: u64) {
    let config = Config::figment()
        .merge(("port", port))
        .merge(("address", IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))))
        .merge(("log_level", "off"));
    let fork_tracker: HashMap<BranchKey, ForkData> =
        HashMap::from([('0', (starting_chain_tip, None))]);
    let fork_tracker_rw_lock = Arc::new(RwLock::new(fork_tracker));
    let _rocket = rocket::build()
        .configure(config)
        .manage(fork_tracker_rw_lock)
        .mount("/", routes![handle_rpc, handle_increment_chain_tip])
        .launch()
        .await
        .unwrap();
}
