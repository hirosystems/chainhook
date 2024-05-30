use crate::service::ScanningData;
use chainhook_sdk::utils::{BlockHeights, BlockHeightsError};
use std::collections::VecDeque;

pub fn get_block_heights_to_scan(
    blocks: &Option<Vec<u64>>,
    start_block: &Option<u64>,
    end_block: &Option<u64>,
    chain_tip: &u64,
    unfinished_scan_data: &Option<ScanningData>,
) -> Result<Option<VecDeque<u64>>, String> {
    let block_heights_to_scan = if let Some(ref blocks) = blocks {
        match BlockHeights::Blocks(blocks.clone()).get_sorted_entries() {
            Ok(heights) => Some(heights),
            Err(e) => match e {
                BlockHeightsError::ExceedsMaxEntries(max, specified) => {
                    return Err(format!("Chainhook specification exceeds max number of blocks to scan. Maximum: {}, Attempted: {}", max, specified));
                }
                BlockHeightsError::StartLargerThanEnd => {
                    // this code path should not be reachable
                    return Err(
                        "Chainhook specification field `end_block` should be greater than `start_block`."
                            .into(),
                    );
                }
            },
        }
    } else {
        let start_block = match &unfinished_scan_data {
            Some(scan_data) => scan_data.last_evaluated_block_height,
            None => start_block.unwrap_or(0),
        };

        let end_block = if let Some(end_block) = end_block {
            if &start_block > end_block {
                return Err(
                    "Chainhook specification field `end_block` should be greater than `start_block`."
                        .into(),
                );
            }
            end_block
        } else {
            chain_tip
        };
        if &start_block > end_block {
            return Ok(None);
        }
        let block_heights_to_scan = match BlockHeights::BlockRange(start_block, *end_block)
            .get_sorted_entries()
        {
            Ok(heights) => heights,
            Err(e) => match e {
                BlockHeightsError::ExceedsMaxEntries(max, specified) => {
                    return Err(format!("Chainhook specification exceeds max number of blocks to scan. Maximum: {}, Attempted: {}", max, specified));
                }
                BlockHeightsError::StartLargerThanEnd => {
                    return Err(
                        "Chainhook specification field `end_block` should be greater than `start_block`."
                            .into(),
                    );
                }
            },
        };
        Some(block_heights_to_scan)
    };
    Ok(block_heights_to_scan)
}

pub enum PredicateScanResult {
    ChainTipReached,
    Expired,
    Derigistered,
}
