use chainhook_types::StacksTransactionEvent;

use crate::indexer::stacks::NewEvent;

pub fn create_new_event_from_stacks_event(event: StacksTransactionEvent) -> NewEvent {
    let mut event_type = String::new();
    let stx_transfer_event = if let StacksTransactionEvent::STXTransferEvent(data) = &event {
        event_type = format!("stx_transfer");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let stx_mint_event = if let StacksTransactionEvent::STXMintEvent(data) = &event {
        event_type = format!("stx_mint");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let stx_burn_event = if let StacksTransactionEvent::STXBurnEvent(data) = &event {
        event_type = format!("stx_burn");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let stx_lock_event = if let StacksTransactionEvent::STXLockEvent(data) = &event {
        event_type = format!("stx_lock");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let nft_transfer_event = if let StacksTransactionEvent::NFTTransferEvent(data) = &event {
        event_type = format!("nft_transfer");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let nft_mint_event = if let StacksTransactionEvent::NFTMintEvent(data) = &event {
        event_type = format!("nft_mint");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let nft_burn_event = if let StacksTransactionEvent::NFTBurnEvent(data) = &event {
        event_type = format!("nft_burn");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let ft_transfer_event = if let StacksTransactionEvent::FTTransferEvent(data) = &event {
        event_type = format!("ft_transfer");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let ft_mint_event = if let StacksTransactionEvent::FTMintEvent(data) = &event {
        event_type = format!("ft_mint");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let ft_burn_event = if let StacksTransactionEvent::FTBurnEvent(data) = &event {
        event_type = format!("ft_burn");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let data_var_set_event = if let StacksTransactionEvent::DataVarSetEvent(data) = &event {
        event_type = format!("data_var_set");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let data_map_insert_event = if let StacksTransactionEvent::DataMapInsertEvent(data) = &event {
        event_type = format!("data_map_insert");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let data_map_update_event = if let StacksTransactionEvent::DataMapUpdateEvent(data) = &event {
        event_type = format!("data_map_update");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let data_map_delete_event = if let StacksTransactionEvent::DataMapDeleteEvent(data) = &event {
        event_type = format!("data_map_delete");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    let contract_event = if let StacksTransactionEvent::SmartContractEvent(data) = &event {
        event_type = format!("smart_contract_print_event");
        Some(serde_json::to_value(data).unwrap())
    } else {
        None
    };
    NewEvent {
        txid: format!(""),
        committed: false,
        event_index: 0,
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
        data_var_set_event,
        data_map_insert_event,
        data_map_update_event,
        data_map_delete_event,
        contract_event,
    }
}
