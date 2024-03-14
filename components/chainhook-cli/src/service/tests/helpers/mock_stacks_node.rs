use crate::scan::stacks::{Record, RecordKind};
use crate::service::tests::helpers::mock_bitcoin_rpc::TipData;
use chainhook_sdk::indexer::bitcoin::NewBitcoinBlock;
use chainhook_sdk::indexer::stacks::{NewBlock, NewEvent, NewTransaction};
use chainhook_sdk::types::{
    FTBurnEventData, FTMintEventData, FTTransferEventData, NFTBurnEventData, NFTMintEventData,
    NFTTransferEventData, STXBurnEventData, STXLockEventData, STXMintEventData,
    STXTransferEventData, SmartContractEventData, StacksTransactionEventPayload,
};

use super::{branch_and_height_to_prefixed_hash, make_block_hash};

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
fn create_stacks_new_event(
    tx_index: u64,
    index: u32,
    event: StacksTransactionEventPayload,
) -> NewEvent {
    let mut event_type = String::new();
    let stx_transfer_event = if let StacksTransactionEventPayload::STXTransferEvent(data) = &event {
        event_type = format!("stx_transfer");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let stx_mint_event = if let StacksTransactionEventPayload::STXMintEvent(data) = &event {
        event_type = format!("stx_mint");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let stx_burn_event = if let StacksTransactionEventPayload::STXBurnEvent(data) = &event {
        event_type = format!("stx_burn");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let stx_lock_event = if let StacksTransactionEventPayload::STXLockEvent(data) = &event {
        event_type = format!("stx_lock");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let nft_transfer_event = if let StacksTransactionEventPayload::NFTTransferEvent(data) = &event {
        event_type = format!("nft_transfer");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let nft_mint_event = if let StacksTransactionEventPayload::NFTMintEvent(data) = &event {
        event_type = format!("nft_mint");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let nft_burn_event = if let StacksTransactionEventPayload::NFTBurnEvent(data) = &event {
        event_type = format!("nft_burn");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let ft_transfer_event = if let StacksTransactionEventPayload::FTTransferEvent(data) = &event {
        event_type = format!("ft_transfer");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let ft_mint_event = if let StacksTransactionEventPayload::FTMintEvent(data) = &event {
        event_type = format!("ft_mint");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let ft_burn_event = if let StacksTransactionEventPayload::FTBurnEvent(data) = &event {
        event_type = format!("ft_burn");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let contract_event = if let StacksTransactionEventPayload::SmartContractEvent(data) = &event {
        event_type = format!("smart_contract_print_event");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    NewEvent {
        txid: format!("transaction_id_{tx_index}"),
        committed: false,
        event_index: index,
        event_type,
        stx_transfer_event,
        stx_mint_event,
        stx_burn_event,
        stx_lock_event,
        nft_transfer_event,
        nft_mint_event,
        nft_burn_event,
        ft_transfer_event,
        ft_mint_event,
        ft_burn_event,
        data_var_set_event: None,
        data_map_insert_event: None,
        data_map_update_event: None,
        data_map_delete_event: None,
        contract_event,
    }
}

fn create_stacks_new_transaction(index: u64) -> NewTransaction {
    NewTransaction {
        txid: format!("transaction_id_{index}"),
        tx_index: index as usize,
        status: format!("success"),
        raw_result: format!("0x0703"),
        raw_tx: format!("0x00000000010400e2cd0871da5bdd38c4d5569493dc3b14aac4e0a10000000000000019000000000000000000008373b16e4a6f9d87864c314dd77bbd8b27a2b1805e96ec5a6509e7e4f833cd6a7bdb2462c95f6968a867ab6b0e8f0a6498e600dbc46cfe9f84c79709da7b9637010200000000040000000000000000000000000000000000000000000000000000000000000000"),
        execution_cost: None,
        contract_abi: None
    }
}

pub fn create_stacks_new_block(
    fork_id: u8,
    height: u64,
    parent_fork_id: u8,
    burn_block_height: u64,
) -> NewBlock {
    let parent_height = match height {
        0 => 0,
        _ => height - 1,
    };
    let parent_burn_block_height = match burn_block_height {
        0 => 0,
        _ => burn_block_height - 1,
    };

    let mut events = vec![];
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::STXTransferEvent(STXTransferEventData {
            sender: format!(""),
            recipient: format!(""),
            amount: format!("1"),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::STXMintEvent(STXMintEventData {
            recipient: format!(""),
            amount: format!("1"),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::STXBurnEvent(STXBurnEventData {
            sender: format!(""),
            amount: format!("1"),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::STXLockEvent(STXLockEventData {
            locked_amount: format!("1"),
            unlock_height: format!(""),
            locked_address: format!(""),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::NFTTransferEvent(NFTTransferEventData {
            asset_class_identifier: format!(""),
            hex_asset_identifier: format!(""),
            sender: format!(""),
            recipient: format!(""),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::NFTMintEvent(NFTMintEventData {
            asset_class_identifier: format!(""),
            hex_asset_identifier: format!(""),
            recipient: format!(""),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::NFTBurnEvent(NFTBurnEventData {
            asset_class_identifier: format!(""),
            hex_asset_identifier: format!(""),
            sender: format!(""),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::FTTransferEvent(FTTransferEventData {
            asset_class_identifier: format!(""),
            sender: format!(""),
            recipient: format!(""),
            amount: format!("1"),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::FTMintEvent(FTMintEventData {
            asset_class_identifier: format!(""),
            recipient: format!(""),
            amount: format!("1"),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::FTBurnEvent(FTBurnEventData {
            asset_class_identifier: format!(""),
            sender: format!(""),
            amount: format!("1"),
        }),
    ));
    events.push(create_stacks_new_event(
        0,
        events.len() as u32,
        StacksTransactionEventPayload::SmartContractEvent(SmartContractEventData {
            contract_identifier: format!(""),
            topic: format!("print"),
            hex_value: format!(""),
        }),
    ));
    NewBlock {
        block_height: height,
        block_hash: make_block_hash(fork_id, height),
        index_block_hash: make_block_hash(fork_id, height),
        burn_block_height: burn_block_height,
        burn_block_hash: make_block_hash(0, burn_block_height),
        parent_block_hash: make_block_hash(parent_fork_id, parent_height),
        parent_index_block_hash: make_block_hash(parent_fork_id, parent_height),
        parent_microblock: "0x0000000000000000000000000000000000000000000000000000000000000000"
            .into(),
        parent_microblock_sequence: 0,
        parent_burn_block_hash: make_block_hash(0, parent_burn_block_height),
        parent_burn_block_height: burn_block_height,
        parent_burn_block_timestamp: 0,
        transactions: (0..4).map(|i| create_stacks_new_transaction(i)).collect(),
        events,
        matured_miner_rewards: vec![],
    }
}

fn create_stacks_block_received_record(
    fork_id: u8,
    height: u64,
    parent_fork_id: u8,
    burn_block_height: u64,
) -> Result<Record, String> {
    let block = create_stacks_new_block(fork_id, height, parent_fork_id, burn_block_height);
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
            .serialize(create_stacks_block_received_record(0, i, 0, i + 100)?)
            .map_err(|e| format!("failed to write tsv file: {}", e.to_string()))?;
    }
    Ok(())
}

pub async fn mine_stacks_block(
    port: u16,
    fork_id: u8,
    height: u64,
    parent_fork_id: u8,
    burn_block_height: u64,
) -> Result<(), String> {
    let block = create_stacks_new_block(fork_id, height, parent_fork_id, burn_block_height);
    let serialized_block = serde_json::to_string(&block).unwrap();
    let client = reqwest::Client::new();
    let _res = client
        .post(format!("http://localhost:{port}/new_block"))
        .header("content-type", "application/json")
        .body(serialized_block)
        .send()
        .await
        .map_err(|e| format!("failed to send new_block request: {}", e.to_string()))?
        .text()
        .await
        .map_err(|e| {
            format!(
                "failed to parse response for new_block request: {}",
                e.to_string()
            )
        })?;
    Ok(())
}

fn create_new_burn_block(branch: Option<char>, burn_block_height: u64) -> NewBitcoinBlock {
    NewBitcoinBlock {
        burn_block_hash: branch_and_height_to_prefixed_hash(branch, burn_block_height),
        burn_block_height,
        reward_recipients: vec![],
        reward_slot_holders: vec![],
        burn_amount: 0,
    }
}

async fn call_increment_chain_tip(
    bitcoin_rpc_port: u16,
    branch: Option<char>,
    burn_block_height: u64,
    parent_branch_key: Option<char>,
    parent_height_at_fork: Option<u64>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let tip_data = TipData {
        branch: branch.unwrap_or('0'),
        parent_branch_key,
        parent_height_at_fork,
    };
    let res = client
        .post(format!(
            "http://localhost:{bitcoin_rpc_port}/increment-chain-tip"
        ))
        .header("Content-Type", "application/json")
        .json(&serde_json::to_value(tip_data).unwrap())
        .send()
        .await
        .map_err(|e| {
            format!(
                "mock bitcoin rpc endpoint increment-chain-tip failed: {}",
                e.to_string()
            )
        })?
        .text()
        .await
        .map_err(|e| {
            format!(
                "failed to parse response for mock bitcoin rpc increment-chain-tip endpoint: {}",
                e.to_string()
            )
        })?;
    assert_eq!(burn_block_height.to_string(), res);
    Ok(())
}

async fn call_new_burn_block(
    stacks_ingestion_port: u16,
    branch: Option<char>,
    burn_block_height: u64,
) -> Result<(), String> {
    let block = create_new_burn_block(branch, burn_block_height);
    let serialized_block = serde_json::to_string(&block)
        .map_err(|e| format!("failed to serialize burn block: {}", e.to_string()))?;
    let client = reqwest::Client::new();
    let _res = client
        .post(format!(
            "http://localhost:{stacks_ingestion_port}/new_burn_block"
        ))
        .header("content-type", "application/json")
        .body(serialized_block)
        .send()
        .await
        .map_err(|e| format!("failed to send new_burn_block request: {}", e.to_string()))?
        .text()
        .await
        .map_err(|e| {
            format!(
                "failed to parse response for new_burn_block request: {}",
                e.to_string()
            )
        })?;
    Ok(())
}

pub async fn mine_burn_block(
    stacks_ingestion_port: u16,
    bitcoin_rpc_port: u16,
    branch: Option<char>,
    burn_block_height: u64,
) -> Result<(), String> {
    call_increment_chain_tip(bitcoin_rpc_port, branch, burn_block_height, None, None).await?;

    call_new_burn_block(stacks_ingestion_port, branch, burn_block_height).await?;
    Ok(())
}

pub async fn create_burn_fork_at(
    stacks_ingestion_port: u16,
    bitcoin_rpc_port: u16,
    branch: Option<char>,
    burn_block_height: u64,
    fork_branch: char,
    fork_at_height: u64,
) -> Result<(), String> {
    call_increment_chain_tip(
        bitcoin_rpc_port,
        branch,
        burn_block_height,
        Some(fork_branch),
        Some(fork_at_height),
    )
    .await?;

    call_new_burn_block(stacks_ingestion_port, branch, burn_block_height).await?;
    Ok(())
}
