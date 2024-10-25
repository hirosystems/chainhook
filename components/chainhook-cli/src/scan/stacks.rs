use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufRead, BufReader},
    sync::{Arc, RwLock},
};

use crate::{
    archive::download_stacks_dataset_if_required,
    config::{Config, PredicatesApi},
    scan::common::get_block_heights_to_scan,
    service::{
        open_readwrite_predicates_db_conn_or_panic, set_confirmed_expiration_status,
        set_predicate_scanning_status, set_unconfirmed_expiration_status, ScanningData,
    },
    storage::{
        get_last_block_height_inserted, get_last_unconfirmed_block_height_inserted,
        get_stacks_block_at_block_height, insert_entry_in_stacks_blocks, is_stacks_block_present,
        open_readonly_stacks_db_conn_with_retry, open_readwrite_stacks_db_conn,
    },
};
use chainhook_sdk::{dispatcher::{ChainhookOccurrencePayload, Dispatcher}, types::{BlockIdentifier, Chain}};
use chainhook_sdk::{
    chainhooks::stacks::evaluate_stacks_chainhook_on_blocks,
    indexer::{self, stacks::standardize_stacks_serialized_block_header, Indexer},
    utils::Context,
};
use chainhook_sdk::{
    chainhooks::stacks::{
        handle_stacks_hook_action, StacksChainhookInstance, StacksChainhookOccurrence,
        StacksTriggerChainhook,
    },
    utils::{file_append, send_request, AbstractStacksBlock},
};
use rocksdb::DB;

use super::common::PredicateScanResult;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DigestingCommand {
    DigestSeedBlock(BlockIdentifier),
    GarbageCollect,
    Kill,
    Terminate,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    pub id: u64,
    pub created_at: String,
    pub kind: RecordKind,
    pub blob: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum RecordKind {
    #[serde(rename = "/new_block")]
    StacksBlockReceived,
    #[serde(rename = "/new_microblocks")]
    StacksMicroblockReceived,
    #[serde(rename = "/new_burn_block")]
    BitcoinBlockReceived,
    #[serde(rename = "/new_mempool_tx")]
    TransactionAdmitted,
    #[serde(rename = "/drop_mempool_tx")]
    TransactionDropped,
    #[serde(rename = "/attachments/new")]
    AttachmentReceived,
}

/// Calculates the canonical chain of Stacks blocks based on a Stacks node events TSV file. Returns a `VecDeque` structure of
/// block hashes along with the line number where we can find the entire block message within the TSV.
pub async fn get_canonical_fork_from_tsv(
    config: &mut Config,
    start_block: Option<u64>,
    ctx: &Context,
) -> Result<VecDeque<(BlockIdentifier, BlockIdentifier, u64)>, String> {
    let seed_tsv_path = config.expected_local_stacks_tsv_file()?.clone();

    let (record_tx, record_rx) = std::sync::mpsc::channel();

    let mut start_block = start_block.unwrap_or(0);
    info!(
        ctx.expect_logger(),
        "Parsing tsv file to determine canonical fork"
    );
    let parsing_handle = hiro_system_kit::thread_named("Stacks chainstate CSV parsing")
        .spawn(move || {
            let mut reader_builder = csv::ReaderBuilder::default()
                .has_headers(false)
                .delimiter(b'\t')
                .buffer_capacity(8 * (1 << 10))
                .from_path(&seed_tsv_path)
                .expect("unable to create csv reader");

            let mut line: u64 = 0;
            for result in reader_builder.deserialize() {
                line += 1;
                let record: Record = result.unwrap();
                if let RecordKind::StacksBlockReceived = &record.kind {
                    if let Err(_e) = record_tx.send(Some((record, line))) {
                        break;
                    }
                };
            }
            let _ = record_tx.send(None);
        })
        .map_err(|e| format!("unable to spawn thread: {e}"))?;

    let stacks_db = open_readonly_stacks_db_conn_with_retry(&config.expected_cache_path(), 3, ctx)?;
    let canonical_fork = {
        let mut cursor = BlockIdentifier::default();
        let mut tsv_new_blocks = HashMap::new();

        while let Ok(Some((record, line))) = record_rx.recv() {
            let (block_identifier, parent_block_identifier) = match (&record.kind, &record.blob) {
                (RecordKind::StacksBlockReceived, Some(blob)) => {
                    match standardize_stacks_serialized_block_header(blob) {
                        Ok(data) => data,
                        Err(e) => {
                            error!(
                                ctx.expect_logger(),
                                "Failed to standardize stacks header: {e}"
                            );
                            continue;
                        }
                    }
                }
                _ => unreachable!(),
            };

            if start_block > block_identifier.index {
                // don't insert blocks that are already in the db,
                // but do fill any gaps in our data
                if is_stacks_block_present(&block_identifier, 0, &stacks_db)
                    || block_identifier.index == 0
                {
                    continue;
                } else {
                    start_block = block_identifier.index;
                    info!(ctx.expect_logger(), "Found missing block ({start_block}) during tsv parsing; will insert into db",);
                }
            }

            if block_identifier.index > cursor.index {
                cursor = block_identifier.clone();
            }
            tsv_new_blocks.insert(block_identifier, (parent_block_identifier, line));
        }

        let mut canonical_fork = VecDeque::new();
        while cursor.index > 0 {
            let (block_identifer, (parent_block_identifier, line)) =
                match tsv_new_blocks.remove_entry(&cursor) {
                    Some(entry) => entry,
                    None => {
                        warn!(
                            ctx.expect_logger(),
                            "Unable to find block {} with index block hash {} in TSV",
                            cursor.index,
                            cursor.hash
                        );
                        break;
                    }
                };
            cursor = parent_block_identifier.clone();
            canonical_fork.push_front((block_identifer, parent_block_identifier, line));
        }
        canonical_fork
    };
    let _ = parsing_handle.join();

    info!(
        ctx.expect_logger(),
        "Finished parsing tsv file to determine canonical fork"
    );
    Ok(canonical_fork)
}

pub async fn scan_stacks_chainstate_via_rocksdb_using_predicate(
    predicate_spec: &StacksChainhookInstance,
    unfinished_scan_data: Option<ScanningData>,
    stacks_db_conn: &DB,
    dispatcher: Dispatcher<ChainhookOccurrencePayload>,
    config: &Config,
    kill_signal: Option<Arc<RwLock<bool>>>,
    ctx: &Context,
) -> Result<PredicateScanResult, String> {
    let predicate_uuid = &predicate_spec.uuid;
    let mut chain_tip = match get_last_unconfirmed_block_height_inserted(stacks_db_conn, ctx) {
        Some(chain_tip) => chain_tip,
        None => match get_last_block_height_inserted(stacks_db_conn, ctx) {
            Some(chain_tip) => chain_tip,
            None => {
                info!(ctx.expect_logger(), "No blocks inserted in db; cannot determine Stacks chain tip. Skipping scan of predicate {}", predicate_uuid);
                return Ok(PredicateScanResult::ChainTipReached);
            }
        },
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
        None => {
            debug!(
                ctx.expect_logger(),
                "Stacks chainstate scan completed. 0 blocks scanned."
            );
            return Ok(PredicateScanResult::ChainTipReached);
        }
    };

    let mut predicates_db_conn = match config.http_api {
        PredicatesApi::On(ref api_config) => {
            Some(open_readwrite_predicates_db_conn_or_panic(api_config, ctx))
        }
        PredicatesApi::Off => None,
    };

    let proofs = HashMap::new();
    debug!(
        ctx.expect_logger(),
        "Starting predicate evaluation on Stacks blocks for predicate {}", predicate_uuid
    );
    let mut last_block_scanned = BlockIdentifier::default();
    let mut err_count = 0;

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
            if number_of_blocks_scanned % 1000 == 0
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
            chain_tip = match get_last_unconfirmed_block_height_inserted(stacks_db_conn, ctx) {
                Some(chain_tip) => chain_tip,
                None => match get_last_block_height_inserted(stacks_db_conn, ctx) {
                    Some(chain_tip) => chain_tip,
                    None => {
                        warn!(ctx.expect_logger(), "No blocks inserted in db; cannot determine Stacks chain tip. Skipping scan of predicate {}", predicate_uuid);
                        return Ok(PredicateScanResult::ChainTipReached);
                    }
                },
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

        let block_data =
            match get_stacks_block_at_block_height(current_block_height, true, 3, stacks_db_conn) {
                Ok(Some(block)) => block,
                Ok(None) => match get_stacks_block_at_block_height(
                    current_block_height,
                    false,
                    3,
                    stacks_db_conn,
                ) {
                    Ok(Some(block)) => block,
                    Ok(None) => {
                        return Err(format!("Unable to retrieve block {current_block_height}"))
                    }
                    Err(e) => {
                        return Err(format!(
                            "Unable to retrieve block {current_block_height}: {e}"
                        ))
                    }
                },
                Err(e) => {
                    return Err(format!(
                        "Unable to retrieve block {current_block_height}: {e}"
                    ))
                }
            };
        last_block_scanned = block_data.block_identifier.clone();

        let blocks: Vec<&dyn AbstractStacksBlock> = vec![&block_data];

        let (hits_per_blocks, _predicates_expired) =
            evaluate_stacks_chainhook_on_blocks(blocks, predicate_spec, ctx);

        if hits_per_blocks.is_empty() {
            continue;
        }

        let trigger = StacksTriggerChainhook {
            chainhook: predicate_spec,
            apply: hits_per_blocks,
            rollback: vec![],
        };
        let res = match handle_stacks_hook_action(
            trigger,
            &proofs,
            &config.get_event_observer_config(),
            ctx,
        ) {
            Err(e) => {
                warn!(
                    ctx.expect_logger(),
                    "unable to handle action for predicate {}: {}", predicate_uuid, e
                );
                Ok(()) // todo: should this error increment our err_count?
            }
            Ok(action) => {
                number_of_times_triggered += 1;
                loop_did_trigger = true;
                let res = match action {
                    StacksChainhookOccurrence::Http(request, data) => {
                        dispatcher.send(request, ChainhookOccurrencePayload::Stacks(data));
                        Ok(())
                        //send_request(request, 3, 1, ctx).await
                    }
                    StacksChainhookOccurrence::File(path, bytes) => file_append(path, bytes, ctx),
                    StacksChainhookOccurrence::Data(_payload) => Ok(()),
                };
                match res {
                    Err(e) => {
                        err_count += 1;
                        Err(e)
                    }
                    Ok(_) => {
                        err_count = 0;
                        Ok(())
                    }
                }
            }
        };
        // We abort after 3 consecutive errors
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
        "Predicate {predicate_uuid} scan completed. {number_of_blocks_scanned} blocks scanned, {number_of_times_triggered} blocks triggering predicate.",
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
            let is_confirmed = match get_stacks_block_at_block_height(
                last_block_scanned.index,
                true,
                3,
                stacks_db_conn,
            ) {
                Ok(block) => block.is_some(),
                Err(e) => {
                    warn!(
                        ctx.expect_logger(),
                        "Failed to get stacks block for status update: {}",
                        e.to_string()
                    );
                    false
                }
            };
            set_unconfirmed_expiration_status(
                &Chain::Stacks,
                number_of_blocks_scanned,
                last_block_scanned.index,
                &predicate_spec.key(),
                predicates_db_conn,
                ctx,
            );
            if is_confirmed {
                set_confirmed_expiration_status(&predicate_spec.key(), predicates_db_conn, ctx);
            }
        }
        return Ok(PredicateScanResult::Expired);
    }

    Ok(PredicateScanResult::ChainTipReached)
}

pub async fn scan_stacks_chainstate_via_csv_using_predicate(
    predicate_spec: &StacksChainhookInstance,
    config: &mut Config,
    ctx: &Context,
) -> Result<BlockIdentifier, String> {
    let start_block = predicate_spec.start_block.unwrap_or_default();
    if let Some(end_block) = predicate_spec.end_block {
        if start_block > end_block {
            return Err(
                "Chainhook specification field `end_block` should be greater than `start_block`."
                    .into(),
            );
        }
    }

    let _ = download_stacks_dataset_if_required(config, ctx).await?;

    let mut canonical_fork = get_canonical_fork_from_tsv(config, None, ctx).await?;

    let mut indexer = Indexer::new(config.network.clone());

    let proofs = HashMap::new();

    let mut occurrences_found = 0;
    let mut blocks_scanned = 0;
    info!(
        ctx.expect_logger(),
        "Starting predicate evaluation on Stacks blocks"
    );
    let mut last_block_scanned = BlockIdentifier::default();
    let mut err_count = 0;
    let tsv_path = config.expected_local_stacks_tsv_file()?.clone();
    let mut tsv_reader = BufReader::new(File::open(tsv_path).map_err(|e| e.to_string())?);
    let mut tsv_current_line = 0;
    for (block_identifier, _parent_block_identifier, tsv_line_number) in canonical_fork.drain(..) {
        if block_identifier.index < start_block {
            continue;
        }
        if let Some(end_block) = predicate_spec.end_block {
            if block_identifier.index > end_block {
                break;
            }
        }

        // Seek to required line from TSV and retrieve its block payload.
        let mut tsv_line = String::new();
        while tsv_current_line < tsv_line_number {
            tsv_line.clear();
            let bytes_read = tsv_reader
                .read_line(&mut tsv_line)
                .map_err(|e| e.to_string())?;
            if bytes_read == 0 {
                return Err("Unexpected EOF when reading TSV".to_string());
            }
            tsv_current_line += 1;
        }
        let Some(serialized_block) = tsv_line.split('\t').last() else {
            return Err("Unable to retrieve serialized block from TSV line".to_string());
        };

        last_block_scanned = block_identifier;
        blocks_scanned += 1;
        let block_data = match indexer::stacks::standardize_stacks_serialized_block(
            &indexer.config,
            serialized_block,
            &mut indexer.stacks_context,
            ctx,
        ) {
            Ok(block) => block,
            Err(e) => {
                error!(&ctx.expect_logger(), "{e}");
                continue;
            }
        };

        let blocks: Vec<&dyn AbstractStacksBlock> = vec![&block_data];

        let (hits_per_blocks, _predicates_expired) =
            evaluate_stacks_chainhook_on_blocks(blocks, predicate_spec, ctx);
        if hits_per_blocks.is_empty() {
            continue;
        }

        let trigger = StacksTriggerChainhook {
            chainhook: predicate_spec,
            apply: hits_per_blocks,
            rollback: vec![],
        };
        match handle_stacks_hook_action(trigger, &proofs, &config.get_event_observer_config(), ctx)
        {
            Err(e) => {
                error!(ctx.expect_logger(), "unable to handle action {}", e);
            }
            Ok(action) => {
                occurrences_found += 1;
                let res = match action {
                    StacksChainhookOccurrence::Http(request, _) => {
                        send_request(request, 10, 3, ctx).await
                    }
                    StacksChainhookOccurrence::File(path, bytes) => file_append(path, bytes, ctx),
                    StacksChainhookOccurrence::Data(_payload) => unreachable!(),
                };
                if res.is_err() {
                    err_count += 1;
                } else {
                    err_count = 0;
                }
            }
        }
        // We abort after 3 consecutive errors
        if err_count >= 3 {
            return Err("Scan aborted (consecutive action errors >= 3)".to_string());
        }
    }
    info!(
        ctx.expect_logger(),
        "{blocks_scanned} blocks scanned, {occurrences_found} occurrences found"
    );

    Ok(last_block_scanned)
}

pub async fn consolidate_local_stacks_chainstate_using_csv(
    config: &mut Config,
    ctx: &Context,
) -> Result<(), String> {
    info!(
        ctx.expect_logger(),
        "Building local chainstate from Stacks archive file"
    );

    let downloaded_new_dataset = download_stacks_dataset_if_required(config, ctx).await?;
    if downloaded_new_dataset {
        let stacks_db =
            open_readonly_stacks_db_conn_with_retry(&config.expected_cache_path(), 3, ctx)?;
        let confirmed_tip = get_last_block_height_inserted(&stacks_db, ctx);
        let mut canonical_fork: VecDeque<(BlockIdentifier, BlockIdentifier, u64)> =
            get_canonical_fork_from_tsv(config, confirmed_tip, ctx).await?;

        let mut indexer = Indexer::new(config.network.clone());
        let mut blocks_inserted = 0;
        let mut blocks_read = 0;
        let blocks_to_insert = canonical_fork.len();
        let stacks_db_rw = open_readwrite_stacks_db_conn(&config.expected_cache_path(), ctx)?;
        info!(
            ctx.expect_logger(),
            "Beginning import of {} Stacks blocks into rocks db", blocks_to_insert
        );
        // TODO: To avoid repeating code with `scan_stacks_chainstate_via_csv_using_predicate`, we should move this block
        // retrieval code into a reusable function.
        let tsv_path = config.expected_local_stacks_tsv_file()?.clone();
        let mut tsv_reader = BufReader::new(File::open(tsv_path).map_err(|e| e.to_string())?);
        let mut tsv_current_line = 0;
        for (block_identifier, _parent_block_identifier, tsv_line_number) in
            canonical_fork.drain(..)
        {
            blocks_read += 1;

            // If blocks already stored, move on
            if is_stacks_block_present(&block_identifier, 3, &stacks_db_rw) {
                continue;
            }
            blocks_inserted += 1;

            // Seek to required line from TSV and retrieve its block payload.
            let mut tsv_line = String::new();
            while tsv_current_line < tsv_line_number {
                tsv_line.clear();
                let bytes_read = tsv_reader
                    .read_line(&mut tsv_line)
                    .map_err(|e| e.to_string())?;
                if bytes_read == 0 {
                    return Err("Unexpected EOF when reading TSV".to_string());
                }
                tsv_current_line += 1;
            }
            let Some(serialized_block) = tsv_line.split('\t').last() else {
                return Err("Unable to retrieve serialized block from TSV line".to_string());
            };

            let block_data = match indexer::stacks::standardize_stacks_serialized_block(
                &indexer.config,
                serialized_block,
                &mut indexer.stacks_context,
                ctx,
            ) {
                Ok(block) => block,
                Err(e) => {
                    error!(
                        &ctx.expect_logger(),
                        "Failed to standardize stacks block: {e}"
                    );
                    continue;
                }
            };

            insert_entry_in_stacks_blocks(&block_data, &stacks_db_rw, ctx)?;

            if blocks_inserted % 2500 == 0 {
                info!(
                    ctx.expect_logger(),
                    "Importing Stacks blocks into rocks db: {}/{}", blocks_read, blocks_to_insert
                );
                let _ = stacks_db_rw.flush();
            }
        }
        let _ = stacks_db_rw.flush();
        info!(
            ctx.expect_logger(),
            "{blocks_read} Stacks blocks read, {blocks_inserted} inserted"
        );
    } else {
        info!(
            ctx.expect_logger(),
            "Skipping database consolidation - no new archive found since last consolidation."
        );
    }
    Ok(())
}
