use std::collections::BTreeMap;

use bitcoin::BlockHash;
use bitcoin_scanner::Scanner;
use chainhook_sdk::{
    bitcoin::Amount,
    bitcoincore_rpc::{Auth, Client, RpcApi},
    bitcoincore_rpc_json::GetRawTransactionResultVoutScriptPubKey,
    indexer::{
        self,
        bitcoin::{
            build_http_client, download_and_parse_block_with_retry, retrieve_block_hash_with_retry,
            BitcoinBlockFullBreakdown, BitcoinTransactionFullBreakdown,
            BitcoinTransactionInputFullBreakdown, BitcoinTransactionInputPrevoutFullBreakdown,
            BitcoinTransactionOutputFullBreakdown, GetRawTransactionResultVinScriptSig,
        },
        BitcoinHistorySource,
    },
    observer::EventObserverConfig,
    types::BitcoinBlockData,
    utils::Context,
};
use reqwest::Client as HttpClient;

use crate::config::Config;

pub fn get_chain_tip(db_dir: &str) -> Result<u64, String> {
    let mut scanner = Scanner::new(db_dir.into());
    let tip_hash = scanner.tip_hash;
    let record = scanner.block_index_record(&tip_hash);
    println!("found height: {}", record.height);
    Ok(record.height as u64)
}

pub struct RpcDataAccess {
    pub bitcoin_rpc: Client,
    pub http_client: HttpClient,
}

pub struct FsDataAccess {
    pub scanner: Scanner,
    pub chain_tip: u64,
    // todo: ideally we store BlockHash, but the scanner mod and chainhook use different versions of bitcoin
    pub height_hash_map: BTreeMap<u64, BlockHash>,
}
pub enum BitcoinDbAccess {
    FS(FsDataAccess),
    RPC(RpcDataAccess),
}

impl BitcoinDbAccess {
    pub fn new(config: &Config) -> Result<BitcoinDbAccess, String> {
        match &config.network.bitcoin_history_source {
            BitcoinHistorySource::FS(config) => {
                let mut scanner = Scanner::new(config.bitcoin_db_dir.clone().into());

                let tip_hash = scanner.tip_hash;
                let tip_record = scanner.block_index_record(&tip_hash);
                let tip_height = tip_record.height;
                let mut record = tip_record;

                let block = scanner.read_block_from_record(&record);

                let tx = block.txdata.first().unwrap();
                let result = scanner.chain_state_record(&tx.txid(), 0);
                println!(
                    "fetched chainstate record for txid {} and vout 0",
                    tx.txid().to_string()
                );
                println!(
                    "expected height: {}, actual height: {:?}",
                    tip_height, result
                );

                let mut height_hash_map = BTreeMap::new();
                height_hash_map.insert(tip_height as u64, record.header.block_hash());
                loop {
                    let prev_blockhash = record.header.prev_blockhash;
                    record = scanner.block_index_record(&prev_blockhash);
                    height_hash_map.insert(record.height as u64, record.header.block_hash());
                    if record.height == 0 {
                        break;
                    }
                }
                Ok(BitcoinDbAccess::FS(FsDataAccess {
                    scanner,
                    chain_tip: tip_height as u64,
                    height_hash_map,
                }))
            }
            BitcoinHistorySource::RPC => {
                let auth = Auth::UserPass(
                    config.network.bitcoind_rpc_username.clone(),
                    config.network.bitcoind_rpc_password.clone(),
                );
                let bitcoin_rpc = Client::new(&config.network.bitcoind_rpc_url, auth)
                    .map_err(|e| format!("Bitcoin RPC error: {}", e.to_string()))?;
                let http_client = build_http_client();
                Ok(BitcoinDbAccess::RPC(RpcDataAccess {
                    bitcoin_rpc,
                    http_client,
                }))
            }
        }
    }

    pub fn get_chain_tip(&self) -> Result<u64, String> {
        match self {
            BitcoinDbAccess::FS(FsDataAccess { chain_tip, .. }) => Ok(chain_tip.clone()),
            BitcoinDbAccess::RPC(RpcDataAccess { bitcoin_rpc, .. }) => {
                match bitcoin_rpc.get_blockchain_info() {
                    Ok(result) => Ok(result.blocks),
                    Err(e) => Err(format!(
                        "unable to retrieve Bitcoin chain tip ({})",
                        e.to_string()
                    )),
                }
            }
        }
    }

    pub async fn get_block(
        &mut self,
        event_observer_config: &EventObserverConfig,
        block_height: &u64,
        ctx: &Context,
    ) -> Result<(BitcoinBlockData, i32), String> {
        let (block_hash, block_breakdown) = match self {
            BitcoinDbAccess::FS(FsDataAccess {
                scanner,
                height_hash_map,
                chain_tip,
            }) => {
                let Some(block_hash) = height_hash_map.get(block_height) else {
                    return Err(format!(
                        "could not find block {} in bitcoin db",
                        block_height
                    ));
                };
                let block_hash_str = block_hash.to_string();
                let block_record = scanner.block_index_record(&block_hash);

                let block = scanner.read_block_from_record(&block_record);

                for tx in block.txdata.iter() {
                    for input in tx.input.iter() {
                        let result = scanner.chain_state_record(
                            &input.previous_output.txid,
                            input.previous_output.vout as u64,
                        );
                        println!(
                            "fetched chainstate record for txid {} and vout {}",
                            input.previous_output.txid, input.previous_output.vout
                        );
                        println!(
                            "expected height: {}, actual height: {:?}",
                            block_height, result
                        );
                    }
                }

                let block_breakdown = BitcoinBlockFullBreakdown {
                    hash: block_hash_str.clone(),
                    height: block_record.height as usize,
                    tx: block
                        .txdata
                        .iter()
                        .map(|tx| {
                            let is_coinbase = tx.is_coinbase(); // todo
                            BitcoinTransactionFullBreakdown {
                                txid: tx.txid().to_string(),
                                vin: tx
                                    .input
                                    .iter()
                                    .map(|input| BitcoinTransactionInputFullBreakdown {
                                        sequence: input.sequence.0,
                                        txid: if is_coinbase {
                                            Some(tx.txid().to_string())
                                        } else {
                                            None
                                        },
                                        vout: if is_coinbase {
                                            Some(input.previous_output.vout)
                                        } else {
                                            None
                                        },
                                        script_sig: if is_coinbase {
                                            Some(GetRawTransactionResultVinScriptSig {
                                                hex: input.script_sig.to_hex_string(),
                                            })
                                        } else {
                                            None
                                        },
                                        txinwitness: if is_coinbase {
                                            Some(
                                                input
                                                    .witness
                                                    .to_vec()
                                                    .iter()
                                                    .map(|w| format!("{:02X?}", w))
                                                    .collect(),
                                            )
                                        } else {
                                            None
                                        },
                                        prevout: if is_coinbase {
                                            Some(BitcoinTransactionInputPrevoutFullBreakdown {
                                                height: 0,
                                                value: Amount::from_sat(0),
                                            })
                                        } else {
                                            None
                                        },
                                    })
                                    .collect(),
                                vout: tx
                                    .output
                                    .iter()
                                    .enumerate()
                                    .map(|(i, output)| BitcoinTransactionOutputFullBreakdown {
                                        value: output.value,
                                        n: i as u32,
                                        script_pub_key: GetRawTransactionResultVoutScriptPubKey {
                                            asm: output.script_pubkey.to_asm_string(),
                                            hex: output.script_pubkey.to_bytes(),
                                            req_sigs: None,
                                            type_: None,
                                            addresses: vec![],
                                            address: None,
                                        },
                                    })
                                    .collect(),
                            }
                        })
                        .collect(),
                    time: block.header.time as usize,
                    nonce: block.header.nonce,
                    previousblockhash: Some(block.header.prev_blockhash.to_string()),
                    confirmations: chain_tip.saturating_sub(*block_height) as i32,
                };

                (block_hash_str.clone(), block_breakdown)
            }
            BitcoinDbAccess::RPC(RpcDataAccess { http_client, .. }) => {
                let block_hash = retrieve_block_hash_with_retry(
                    &http_client,
                    block_height,
                    &event_observer_config.get_bitcoin_config(),
                    ctx,
                )
                .await?;
                let block_breakdown = download_and_parse_block_with_retry(
                    &http_client,
                    &block_hash,
                    &event_observer_config.get_bitcoin_config(),
                    ctx,
                )
                .await?;
                let block_hash = block_hash.clone();
                (block_hash, block_breakdown)
            }
        };
        let confirmations = block_breakdown.confirmations;
        let block = match indexer::bitcoin::standardize_bitcoin_block(
            block_breakdown,
            &event_observer_config.bitcoin_network,
            ctx,
        ) {
            Ok(data) => Ok(data),
            Err((e, _)) => Err(format!(
                "Unable to standardize block #{} {}: {}",
                block_height, block_hash, e
            )),
        }?;
        Ok((block, confirmations))
    }
}
