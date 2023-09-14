use crate::scan::stacks::{Record, RecordKind};
use chainhook_sdk::indexer::bitcoin::NewBitcoinBlock;
use chainhook_sdk::indexer::stacks::{NewBlock, NewTransaction};

use super::height_to_prefixed_hash;

pub const TEST_WORKING_DIR: &str = "src/service/tests/fixtures/tmp";

pub fn create_tmp_working_dir() -> Result<(String, String), String> {
    let mut rng = rand::thread_rng();
    let random_digit: u64 = rand::Rng::gen(&mut rng);
    let working_dir = format!("{TEST_WORKING_DIR}/{random_digit}");
    let tsv_dir = format!("./{working_dir}/stacks_blocks.tsv");
    std::fs::create_dir_all(&working_dir)
        .map_err(|e| format!("failed to create temp working dir: {}", e.to_string()))?;
    Ok((working_dir, tsv_dir))
}

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

fn create_stacks_block_received_record(
    height: u64,
    burn_block_height: u64,
) -> Result<Record, String> {
    let block = create_stacks_new_block(height, burn_block_height);
    let serialized_block = serde_json::to_string(&block)
        .map_err(|e| format!("failed to serialize stacks block: {}", e.to_string()))?;
    Ok(Record {
        id: height,
        created_at: height.to_string(),
        kind: RecordKind::StacksBlockReceived,
        blob: Some(serialized_block),
    })
}
pub fn write_stacks_blocks_to_tsv(block_count: u64, dir: &str) -> Result<(), String> {
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
            .serialize(create_stacks_block_received_record(i, i + 100)?)
            .map_err(|e| format!("failed to write tsv file: {}", e.to_string()))?;
    }
    Ok(())
}

pub async fn mine_stacks_block(port: u16, height: u64, burn_block_height: u64) {
    let block = create_stacks_new_block(height, burn_block_height);
    let serialized_block = serde_json::to_string(&block).unwrap();
    let client = reqwest::Client::new();
    let _res = client
        .post(format!("http://localhost:{port}/new_block"))
        .header("content-type", "application/json")
        .body(serialized_block)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
}

fn create_new_burn_block(burn_block_height: u64) -> NewBitcoinBlock {
    NewBitcoinBlock {
        burn_block_hash: height_to_prefixed_hash(burn_block_height),
        burn_block_height,
        reward_recipients: vec![],
        reward_slot_holders: vec![],
        burn_amount: 0,
    }
}

pub async fn mine_burn_block(
    stacks_ingestion_port: u16,
    bitcoin_rpc_port: u16,
    burn_block_height: u64,
) {
    let block = create_new_burn_block(burn_block_height);
    let serialized_block = serde_json::to_string(&block).unwrap();
    let client = reqwest::Client::new();
    let res = client
        .post(format!(
            "http://localhost:{bitcoin_rpc_port}/increment-chain-tip"
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(burn_block_height.to_string(), res);
    let _res = client
        .post(format!(
            "http://localhost:{stacks_ingestion_port}/new_burn_block"
        ))
        .header("content-type", "application/json")
        .body(serialized_block)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
}
