use crate::scan::stacks::Record;
use crate::scan::stacks::RecordKind;
use chainhook_sdk::indexer::stacks::NewBlock;
use chainhook_sdk::indexer::stacks::NewTransaction;

fn create_stacks_tsv_transaction(index: u64) -> NewTransaction {
    NewTransaction {
        txid: format!("transaction_id_{index}"),
        tx_index: index as usize,
        status: format!("success"),
        raw_result: format!("0x0703"),
        raw_tx: format!("0x00000000010400e2cd0871da5bdd38c4d5569493dc3b14aac4e0a10000000000000019000000000000000000008373b16e4a6f9d87864c314dd77bbd8b27a2b1805e96ec5a6509e7e4f833cd6a7bdb2462c95f6968a867ab6b0e8f0a6498e600dbc46cfe9f84c79709da7b9637010200000000040000000000000000000000000000000000000000000000000000000000000000"),
        execution_cost: None,
    }
}

pub fn create_stacks_tsv_block(height: u64, burn_block_height: u64) -> NewBlock {
    let parent_height = if height == 0 { 0 } else { height - 1 };
    let parent_burn_block_height = if burn_block_height == 0 {
        0
    } else {
        burn_block_height - 1
    };

    NewBlock {
        block_height: height,
        block_hash: format!("0x000000000000000000000000000000000000000000000000000000000000000{height}"),
        index_block_hash: format!("0x000000000000000000000000000000000000000000000000000000000000000{height}"),
        burn_block_height: burn_block_height,
        burn_block_hash: format!("0x000000000000000000000000000000000000000000000000000000000000000{burn_block_height}"),
        parent_block_hash: format!("0x000000000000000000000000000000000000000000000000000000000000000{parent_height}"),
        parent_index_block_hash: format!("0x000000000000000000000000000000000000000000000000000000000000000{parent_height}"),
        parent_microblock: "0x0000000000000000000000000000000000000000000000000000000000000000"
            .into(),
        parent_microblock_sequence: 0,
        parent_burn_block_hash: format!("0x000000000000000000000000000000000000000000000000000000000000000{parent_burn_block_height}"),
        parent_burn_block_height: burn_block_height,
        parent_burn_block_timestamp: 0,
        transactions: (0..4).map(|i| create_stacks_tsv_transaction(i)).collect(),
        events: vec![],
        matured_miner_rewards: vec![],
    }
}

fn create_stacks_tsv_block_received_record(height: u64, burn_block_height: u64) -> Record {
    let block = create_stacks_tsv_block(height, burn_block_height);
    let serialized_block = serde_json::to_string(&block).unwrap();
    Record {
        id: height,
        created_at: height.to_string(),
        kind: RecordKind::StacksBlockReceived,
        blob: Some(serialized_block),
    }
}
pub const WORKING_DIR: &str = "src/service/tests/fixtures/tmp";
pub fn create_stacks_tsv_with_blocks(block_count: u64, dir: &str) {
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
            .serialize(create_stacks_tsv_block_received_record(i, i + 100))
            .unwrap();
    }
}
