use crate::config::{Config, PredicatesApi};
use crate::scan::common::get_block_heights_to_scan;
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
use chainhook_sdk::chainhooks::bitcoin::BitcoinChainhookInstance;
use chainhook_sdk::dispatcher::{ChainhookOccurrencePayload, Dispatcher};
use chainhook_sdk::indexer;
use chainhook_sdk::indexer::bitcoin::{
    build_http_client, download_and_parse_block_with_retry, retrieve_block_hash_with_retry,
};
use chainhook_sdk::indexer::fork_scratch_pad::CONFIRMED_SEGMENT_MINIMUM_LENGTH;
use chainhook_sdk::observer::{gather_proofs, EventObserverConfig};
use chainhook_sdk::types::{
    BitcoinBlockData, BitcoinChainEvent, BitcoinChainUpdatedWithBlocksData, BlockIdentifier, Chain,
};
use chainhook_sdk::utils::{file_append, send_request, Context};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::common::PredicateScanResult;

pub async fn scan_bitcoin_chainstate_via_rpc_using_predicate(
    predicate_spec: &BitcoinChainhookInstance,
    unfinished_scan_data: Option<ScanningData>,
    dispatcher: Dispatcher<ChainhookOccurrencePayload>,
    config: &Config,
    kill_signal: Option<Arc<RwLock<bool>>>,
    ctx: &Context,
) -> Result<PredicateScanResult, String> {
    let predicate_uuid = &predicate_spec.uuid;
    let auth = Auth::UserPass(
        config.network.bitcoind_rpc_username.clone(),
        config.network.bitcoind_rpc_password.clone(),
    );

    let bitcoin_rpc = match Client::new(&config.network.bitcoind_rpc_url, auth) {
        Ok(con) => con,
        Err(message) => {
            return Err(format!("Bitcoin RPC error: {}", message));
        }
    };

    let mut chain_tip = match bitcoin_rpc.get_blockchain_info() {
        Ok(result) => result.blocks,
        Err(e) => {
            return Err(format!(
                "unable to retrieve Bitcoin chain tip ({})",
                e
            ));
        }
    };

    let block_heights_to_scan = get_block_heights_to_scan(
        &predicate_spec.blocks,
        &predicate_spec.start_block,
        &predicate_spec.end_block,
        &chain_tip,
        &unfinished_scan_data,
    )?;
    let mut block_heights_to_scan = match block_heights_to_scan {
        Some(h) => h,
        // no blocks to scan, go straight to streaming
        None => return Ok(PredicateScanResult::ChainTipReached),
    };

    let mut predicates_db_conn = match config.http_api {
        PredicatesApi::On(ref api_config) => {
            Some(open_readwrite_predicates_db_conn_or_panic(api_config, ctx))
        }
        PredicatesApi::Off => None,
    };

    debug!(
        ctx.expect_logger(),
        "Starting predicate evaluation on Bitcoin blocks for predicate {predicate_uuid}",
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

    let mut loop_did_trigger = false;
    while let Some(current_block_height) = block_heights_to_scan.pop_front() {
        if let Some(kill_signal) = kill_signal.clone() {
            if let Ok(kill_signal) = kill_signal.read() {
                // if true, we're received the kill signal, so break out of the loop
                if *kill_signal {
                    return Ok(PredicateScanResult::Deregistered);
                }
            }
        }
        if let Some(ref mut predicates_db_conn) = predicates_db_conn {
            if number_of_blocks_scanned % 100 == 0 
                || number_of_blocks_scanned == 0
                // if the last loop did trigger a predicate, update the status
                || loop_did_trigger
            {
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
        loop_did_trigger = false;

        if current_block_height > chain_tip {
            let prev_chain_tip = chain_tip;
            // we've scanned up to the chain tip as of the start of this scan
            // so see if the chain has progressed since then
            chain_tip = match bitcoin_rpc.get_blockchain_info() {
                Ok(result) => result.blocks,
                Err(e) => {
                    return Err(format!(
                        "unable to retrieve Bitcoin chain tip ({})",
                        e
                    ));
                }
            };
            // if the chain hasn't progressed, break out so we can enter streaming mode
            // and put back the block we weren't able to scan
            if current_block_height > chain_tip {
                block_heights_to_scan.push_front(current_block_height);
                break;
            } else {
                // if the chain has progressed, update our total number of blocks to scan and keep scanning
                number_of_blocks_to_scan += chain_tip - prev_chain_tip;
            }
        }

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
            &dispatcher,
            ctx,
        )
        .await
        {
            Ok(actions) => {
                if actions > 0 {
                    number_of_times_triggered += 1;
                    loop_did_trigger = true
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
                return Err("Scan aborted (consecutive action errors >= 3)".to_string());
            }
        }
    }

    info!(
        ctx.expect_logger(),
        "Predicate {predicate_uuid} scan completed. {number_of_blocks_scanned} blocks scanned, {actions_triggered} actions triggered."
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
    }
    // if an end block was provided, or a fixed number of blocks were set to be scanned,
    // check to see if we've processed all of the blocks and can expire the predicate.
    if (predicate_spec.blocks.is_some()
        || (predicate_spec.end_block.is_some()
            && predicate_spec.end_block.unwrap() == last_block_scanned.index))
        && block_heights_to_scan.is_empty()
    {
        if let Some(ref mut predicates_db_conn) = predicates_db_conn {
            set_unconfirmed_expiration_status(
                &Chain::Bitcoin,
                number_of_blocks_scanned,
                last_block_scanned.index,
                &predicate_spec.key(),
                predicates_db_conn,
                ctx,
            );
            if last_scanned_block_confirmations >= CONFIRMED_SEGMENT_MINIMUM_LENGTH {
                set_confirmed_expiration_status(&predicate_spec.key(), predicates_db_conn, ctx);
            }
        }
        return Ok(PredicateScanResult::Expired);
    }

    Ok(PredicateScanResult::ChainTipReached)
}

pub async fn process_block_with_predicates(
    block: BitcoinBlockData,
    predicates: &Vec<&BitcoinChainhookInstance>,
    event_observer_config: &EventObserverConfig,
    dispatcher: &Dispatcher<ChainhookOccurrencePayload>,
    ctx: &Context,
) -> Result<u32, String> {
    let chain_event =
        BitcoinChainEvent::ChainUpdatedWithBlocks(BitcoinChainUpdatedWithBlocksData {
            new_blocks: vec![block],
            confirmed_blocks: vec![],
        });

    let (predicates_triggered, _predicates_evaluated, _predicates_expired) =
        evaluate_bitcoin_chainhooks_on_chain_event(&chain_event, predicates, ctx);

    execute_predicates_action(predicates_triggered, dispatcher, event_observer_config, ctx).await
}

pub async fn execute_predicates_action<'a>(
    hits: Vec<BitcoinTriggerChainhook<'a>>,
    dispatcher: &Dispatcher<ChainhookOccurrencePayload>,
    config: &EventObserverConfig,
    ctx: &Context,
) -> Result<u32, String> {
    let mut actions_triggered = 0;
    let mut proofs = HashMap::new();
    for trigger in hits.into_iter() {
        if trigger.chainhook.include_proof {
            gather_proofs(&trigger, &mut proofs, config, ctx);
        }
        let predicate_uuid = &trigger.chainhook.uuid;
        match handle_bitcoin_hook_action(trigger, &proofs, &config) {
            Err(e) => {
                warn!(
                    ctx.expect_logger(),
                    "unable to handle action for predicate {}: {}", predicate_uuid, e
                );
            }
            Ok(action) => {
                actions_triggered += 1;
                match action {
                    BitcoinChainhookOccurrence::Http(request, data) => {
                        dispatcher.send(request, ChainhookOccurrencePayload::Bitcoin(data));
                        //send_request(request, 10, 3, ctx).await?
                    }
                    BitcoinChainhookOccurrence::File(path, bytes) => {
                        file_append(path, bytes, ctx)?
                    }
                    BitcoinChainhookOccurrence::Data(_payload) => {}
                };
            }
        }
    }

    Ok(actions_triggered)
}
