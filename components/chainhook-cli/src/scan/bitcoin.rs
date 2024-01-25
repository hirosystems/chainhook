use crate::config::{Config, PredicatesApi};
use crate::service::{
    open_readwrite_predicates_db_conn_or_panic, set_confirmed_expiration_status,
    set_predicate_scanning_status, set_unconfirmed_expiration_status, ScanningData,
};
use chainhook_sdk::bitcoincore_rpc::RpcApi;
use chainhook_sdk::bitcoincore_rpc::{Auth, Client};
use chainhook_sdk::chainhooks::bitcoin::{
    evaluate_bitcoin_chainhooks_on_chain_event, handle_bitcoin_hook_action,
    BitcoinChainhookOccurrence, BitcoinTriggerChainhook,
};
use chainhook_sdk::chainhooks::types::BitcoinChainhookSpecification;
use chainhook_sdk::indexer;
use chainhook_sdk::indexer::bitcoin::{
    build_http_client, download_and_parse_block_with_retry, retrieve_block_hash_with_retry,
};
use chainhook_sdk::indexer::fork_scratch_pad::CONFIRMED_SEGMENT_MINIMUM_LENGTH;
use chainhook_sdk::observer::{gather_proofs, EventObserverConfig};
use chainhook_sdk::types::{
    BitcoinBlockData, BitcoinChainEvent, BitcoinChainUpdatedWithBlocksData, BlockIdentifier, Chain,
};
use chainhook_sdk::utils::{file_append, send_request, BlockHeights, Context};
use std::collections::HashMap;

pub async fn scan_bitcoin_chainstate_via_rpc_using_predicate(
    predicate_spec: &BitcoinChainhookSpecification,
    unfinished_scan_data: Option<ScanningData>,
    config: &Config,
    ctx: &Context,
) -> Result<bool, String> {
    let auth = Auth::UserPass(
        config.network.bitcoind_rpc_username.clone(),
        config.network.bitcoind_rpc_password.clone(),
    );

    let bitcoin_rpc = match Client::new(&config.network.bitcoind_rpc_url, auth) {
        Ok(con) => con,
        Err(message) => {
            return Err(format!("Bitcoin RPC error: {}", message.to_string()));
        }
    };
    let mut floating_end_block = false;

    let mut block_heights_to_scan = if let Some(ref blocks) = predicate_spec.blocks {
        // todo: if a user provides a number of blocks where start_block + blocks > chain tip,
        // the predicate will fail to scan all blocks. we should calculate a valid end_block and
        // switch to streaming mode at some point
        BlockHeights::Blocks(blocks.clone()).get_sorted_entries()
    } else {
        let start_block = match predicate_spec.start_block {
            Some(start_block) => match &unfinished_scan_data {
                Some(scan_data) => scan_data.last_evaluated_block_height,
                None => start_block,
            },
            None => {
                return Err(
                    "Bitcoin chainhook specification must include a field start_block in replay mode"
                        .into(),
                );
            }
        };
        let (end_block, update_end_block) = match bitcoin_rpc.get_blockchain_info() {
            Ok(result) => match predicate_spec.end_block {
                Some(end_block) => {
                    if end_block > result.blocks {
                        (result.blocks, true)
                    } else {
                        (end_block, false)
                    }
                }
                None => (result.blocks, true),
            },
            Err(e) => {
                return Err(format!(
                    "unable to retrieve Bitcoin chain tip ({})",
                    e.to_string()
                ));
            }
        };
        floating_end_block = update_end_block;
        BlockHeights::BlockRange(start_block, end_block).get_sorted_entries()
    };

    let mut predicates_db_conn = match config.http_api {
        PredicatesApi::On(ref api_config) => {
            Some(open_readwrite_predicates_db_conn_or_panic(api_config, ctx))
        }
        PredicatesApi::Off => None,
    };

    info!(
        ctx.expect_logger(),
        "Starting predicate evaluation on Bitcoin blocks",
    );

    let mut last_block_scanned = BlockIdentifier::default();
    let mut actions_triggered = 0;
    let mut err_count = 0;

    let event_observer_config = config.get_event_observer_config();
    let bitcoin_config = event_observer_config.get_bitcoin_config();

    let (mut number_of_blocks_to_scan, mut number_of_blocks_scanned, mut number_of_times_triggered) = {
        let number_of_blocks_to_scan = block_heights_to_scan.len() as u64;
        match &unfinished_scan_data {
            Some(scan_data) => (
                scan_data.number_of_blocks_to_scan,
                scan_data.number_of_blocks_evaluated,
                scan_data.number_of_times_triggered,
            ),
            None => (number_of_blocks_to_scan, 0, 0u64),
        }
    };
    let mut last_scanned_block_confirmations = 0;
    let http_client = build_http_client();

    while let Some(current_block_height) = block_heights_to_scan.pop_front() {
        number_of_blocks_scanned += 1;

        let block_hash = retrieve_block_hash_with_retry(
            &http_client,
            &current_block_height,
            &bitcoin_config,
            ctx,
        )
        .await?;
        let block_breakdown =
            download_and_parse_block_with_retry(&http_client, &block_hash, &bitcoin_config, ctx)
                .await?;
        last_scanned_block_confirmations = block_breakdown.confirmations;
        let block = match indexer::bitcoin::standardize_bitcoin_block(
            block_breakdown,
            &event_observer_config.bitcoin_network,
            ctx,
        ) {
            Ok(data) => data,
            Err((e, _)) => {
                warn!(
                    ctx.expect_logger(),
                    "Unable to standardize block #{} {}: {}", current_block_height, block_hash, e
                );
                continue;
            }
        };
        last_block_scanned = block.block_identifier.clone();

        let res = match process_block_with_predicates(
            block,
            &vec![&predicate_spec],
            &event_observer_config,
            ctx,
        )
        .await
        {
            Ok(actions) => {
                if actions > 0 {
                    number_of_times_triggered += 1;
                }
                actions_triggered += actions;
                Ok(())
            }
            Err(e) => {
                err_count += 1;
                Err(e)
            }
        };

        if err_count >= 3 {
            if res.is_err() {
                return Err(format!(
                    "Scan aborted (consecutive action errors >= 3): {}",
                    res.unwrap_err()
                ));
            } else {
                return Err(format!("Scan aborted (consecutive action errors >= 3)"));
            }
        }

        if let Some(ref mut predicates_db_conn) = predicates_db_conn {
            if number_of_blocks_scanned % 10 == 0 || number_of_blocks_scanned == 1 {
                set_predicate_scanning_status(
                    &predicate_spec.key(),
                    number_of_blocks_to_scan,
                    number_of_blocks_scanned,
                    number_of_times_triggered,
                    current_block_height,
                    predicates_db_conn,
                    ctx,
                );
            }
        }

        if block_heights_to_scan.is_empty() && floating_end_block {
            let new_tip = match bitcoin_rpc.get_blockchain_info() {
                Ok(result) => match predicate_spec.end_block {
                    Some(end_block) => {
                        if end_block > result.blocks {
                            result.blocks
                        } else {
                            end_block
                        }
                    }
                    None => result.blocks,
                },
                Err(_e) => {
                    continue;
                }
            };

            for entry in (current_block_height + 1)..new_tip {
                block_heights_to_scan.push_back(entry);
            }
            number_of_blocks_to_scan += block_heights_to_scan.len() as u64;
        }
    }
    info!(
        ctx.expect_logger(),
        "{number_of_blocks_scanned} blocks scanned, {actions_triggered} actions triggered"
    );

    if let Some(ref mut predicates_db_conn) = predicates_db_conn {
        set_predicate_scanning_status(
            &predicate_spec.key(),
            number_of_blocks_to_scan,
            number_of_blocks_scanned,
            number_of_times_triggered,
            last_block_scanned.index,
            predicates_db_conn,
            ctx,
        );
        if let Some(predicate_end_block) = predicate_spec.end_block {
            if predicate_end_block == last_block_scanned.index {
                // todo: we need to find a way to check if this block is confirmed
                // and if so, set the status to confirmed expiration
                set_unconfirmed_expiration_status(
                    &Chain::Bitcoin,
                    number_of_blocks_scanned,
                    predicate_end_block,
                    &predicate_spec.key(),
                    predicates_db_conn,
                    ctx,
                );
                if last_scanned_block_confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH {
                    set_confirmed_expiration_status(&predicate_spec.key(), predicates_db_conn, ctx);
                }
                return Ok(true);
            }
        }
    }

    return Ok(false);
}

pub async fn process_block_with_predicates(
    block: BitcoinBlockData,
    predicates: &Vec<&BitcoinChainhookSpecification>,
    event_observer_config: &EventObserverConfig,
    ctx: &Context,
) -> Result<u32, String> {
    let chain_event =
        BitcoinChainEvent::ChainUpdatedWithBlocks(BitcoinChainUpdatedWithBlocksData {
            new_blocks: vec![block],
            confirmed_blocks: vec![],
        });

    let (predicates_triggered, _predicates_evaluated, _predicates_expired) =
        evaluate_bitcoin_chainhooks_on_chain_event(&chain_event, predicates, ctx);

    execute_predicates_action(predicates_triggered, &event_observer_config, &ctx).await
}

pub async fn execute_predicates_action<'a>(
    hits: Vec<BitcoinTriggerChainhook<'a>>,
    config: &EventObserverConfig,
    ctx: &Context,
) -> Result<u32, String> {
    let mut actions_triggered = 0;
    let mut proofs = HashMap::new();
    for trigger in hits.into_iter() {
        if trigger.chainhook.include_proof {
            gather_proofs(&trigger, &mut proofs, &config, &ctx);
        }
        match handle_bitcoin_hook_action(trigger, &proofs) {
            Err(e) => {
                error!(ctx.expect_logger(), "unable to handle action {}", e);
            }
            Ok(action) => {
                actions_triggered += 1;
                match action {
                    BitcoinChainhookOccurrence::Http(request, _) => {
                        send_request(request, 10, 3, &ctx).await?
                    }
                    BitcoinChainhookOccurrence::File(path, bytes) => {
                        file_append(path, bytes, &ctx)?
                    }
                    BitcoinChainhookOccurrence::Data(_payload) => {}
                };
            }
        }
    }

    Ok(actions_triggered)
}
