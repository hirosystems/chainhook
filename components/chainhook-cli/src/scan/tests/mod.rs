use std::collections::VecDeque;

use test_case::test_case;

use crate::service::ScanningData;

use super::common::get_block_heights_to_scan;

fn expect_exceeded_max_entries_error(
    (result, _expected_entries): (Result<Option<VecDeque<u64>>, String>, Option<VecDeque<u64>>),
) {
    match result {
        Ok(_) => panic!("Expected exceeds max entries error."),
        Err(e) => {
            if !e.contains("exceeds max number") {
                panic!("Expected exceeds max entries error. Received error {}", e);
            }
        }
    };
}

fn expect_start_larger_than_end_error(
    (result, _expected_entries): (Result<Option<VecDeque<u64>>, String>, Option<VecDeque<u64>>),
) {
    match result {
        Ok(_) => panic!("Expected start larger than end error."),
        Err(e) => {
            if !e.contains("field `end_block` should be greater than `start_block`") {
                panic!("Expected start larger than end error. Received error {}", e);
            }
        }
    };
}
fn expect_entries(
    (result, expected_entries): (Result<Option<VecDeque<u64>>, String>, Option<VecDeque<u64>>),
) {
    match result {
        Ok(actual_entries) => {
            match actual_entries {
                None => {
                    if let Some(expected_entries) = expected_entries {
                        panic!("No entries found. Expected: {:?}", expected_entries);
                    }
                }
                Some(actual_entries) => {
                    if let Some(expected_entries) = expected_entries {
                        assert_eq!(
                            actual_entries.len(),
                            expected_entries.len(),
                            "Number of blocks to scan differs from expected. Actual: {:?}, Expected: {:?}",
                            actual_entries,
                            expected_entries
                        );
                        for (i, actual_entry) in actual_entries.iter().enumerate() {
                            let expected_entry = &expected_entries[i];
                            assert_eq!(actual_entry, expected_entry, "Entry different from expected. Actual: {}, Expected: {}, Index: {}", actual_entry, expected_entry, i);
                        }
                    } else {
                        panic!(
                            "Found entries when non were expectd. Actual: {:?}",
                            actual_entries
                        );
                    }
                }
            }
        }
        Err(e) => {
            panic!("Did not exptect getting block heights to error. Recieved error {e}");
        }
    };
}
fn get_huge_vec() -> Vec<u64> {
    let mut vec = vec![];
    for i in 0..1_000_001 {
        vec.push(i);
    }
    vec
}

#[test_case(None, Some(1), Some(2), 3, None, Some(VecDeque::from([1,2])) => using expect_entries; "chain_tip > end_block > start_block yields vec from start to end")]
#[test_case(None, Some(2), Some(1), 3, None, None => using expect_start_larger_than_end_error; "chain_tip > start_block > end_block errors")]
#[test_case(None, Some(1), Some(3), 2, None, Some(VecDeque::from([1,2,3])) => using expect_entries; "end_block > chain_tip > start_block yields vec from start to end")]
#[test_case(None, Some(2), Some(3), 1, None, Some(VecDeque::from([2,3])) => using expect_entries; "end_block > start_block > chain_tip yields vec from start to end")]
#[test_case(None, Some(3), Some(1), 2, None, None => using expect_start_larger_than_end_error; "start_block > chain_tip > end_block errors")]
#[test_case(None, Some(3), Some(2), 1, None, None => using expect_start_larger_than_end_error; "start_block > end_block > chain_tip errors")]
#[test_case(None, None, None, 3, None, Some(VecDeque::from([0,1,2,3])) => using expect_entries; "no end_block, no start_block yields 0 to chain_tip")]
#[test_case(None, Some(3), None, 1, None, None => using expect_entries; "start_block > chain_tip, no end_block yields None")]
#[test_case(None, Some(1), None, 3, None, Some(VecDeque::from([1,2,3])) => using expect_entries; "chain_tip > start_block, no end_block yields vec from start to chain")]
#[test_case(None, None, Some(3), 2, None, Some(VecDeque::from([0,1,2,3])) => using expect_entries; "end_block > chain_tip, no start_block yields vec from 0 to end")]
#[test_case(None, None, Some(2), 3, None, Some(VecDeque::from([0,1,2])) => using expect_entries; "chain_tip > end_block, no yields vec from 0 to end_block")]
#[test_case(None, Some(0), Some(1_000_000_000), 0, None, None => using expect_exceeded_max_entries_error; "limits max number of entries")]
#[test_case(None, Some(0), Some(3), 0, Some(ScanningData { number_of_blocks_to_scan: 0, number_of_blocks_evaluated: 0, number_of_times_triggered: 0, last_occurrence: None, last_evaluated_block_height: 2}), Some(VecDeque::from([2,3])) => using expect_entries; "uses previous scan data for start_block if available")]
#[test_case(Some(vec![0,1,2]), None, None, 0, None, Some(VecDeque::from([0,1,2])) => using expect_entries; "providing blocks returns the same blocks as vec")]
#[test_case(Some(get_huge_vec()), None, None, 0, None, None => using expect_exceeded_max_entries_error; "providing too many blocks errors")]
fn test_get_block_heights_to_scan(
    blocks: Option<Vec<u64>>,
    start_block: Option<u64>,
    end_block: Option<u64>,
    chain_tip: u64,
    unfinished_scan_data: Option<ScanningData>,
    expected: Option<VecDeque<u64>>,
) -> (Result<Option<VecDeque<u64>>, String>, Option<VecDeque<u64>>) {
    (
        get_block_heights_to_scan(
            &blocks,
            &start_block,
            &end_block,
            &chain_tip,
            &unfinished_scan_data,
        ),
        expected,
    )
}
