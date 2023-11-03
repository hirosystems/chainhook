use chainhook_types::{
    DataMapDeleteEventData, DataMapInsertEventData, DataMapUpdateEventData, DataVarSetEventData,
    FTBurnEventData, FTMintEventData, FTTransferEventData, NFTBurnEventData, NFTMintEventData,
    NFTTransferEventData, STXBurnEventData, STXLockEventData, STXMintEventData,
    STXTransferEventData, SmartContractEventData, StacksTransactionEvent,
};

use crate::indexer::tests::helpers::stacks_events::create_new_event_from_stacks_event;

use super::{
    super::tests::{helpers, process_stacks_blocks_and_check_expectations},
    NewEvent,
};
use test_case::test_case;

#[test]
fn test_stacks_vector_001() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_001());
}

#[test]
fn test_stacks_vector_002() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_002());
}

#[test]
fn test_stacks_vector_003() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_003());
}

#[test]
fn test_stacks_vector_004() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_004());
}

#[test]
fn test_stacks_vector_005() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_005());
}

#[test]
fn test_stacks_vector_006() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_006());
}

#[test]
fn test_stacks_vector_007() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_007());
}

#[test]
fn test_stacks_vector_008() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_008());
}

#[test]
fn test_stacks_vector_009() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_009());
}

#[test]
fn test_stacks_vector_010() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_010());
}

#[test]
fn test_stacks_vector_011() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_011());
}

#[test]
fn test_stacks_vector_012() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_012());
}

#[test]
fn test_stacks_vector_013() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_013());
}

#[test]
fn test_stacks_vector_014() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_014());
}

#[test]
fn test_stacks_vector_015() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_015());
}

#[test]
fn test_stacks_vector_016() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_016());
}

#[test]
fn test_stacks_vector_017() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_017());
}

#[test]
fn test_stacks_vector_018() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_018());
}

#[test]
fn test_stacks_vector_019() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_019());
}

#[test]
fn test_stacks_vector_020() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_020());
}

#[test]
fn test_stacks_vector_021() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_021());
}

#[test]
fn test_stacks_vector_022() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_022());
}

#[test]
fn test_stacks_vector_023() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_023());
}

#[test]
fn test_stacks_vector_024() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_024());
}

#[test]
fn test_stacks_vector_025() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_025());
}

#[test]
fn test_stacks_vector_026() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_026());
}

#[test]
fn test_stacks_vector_027() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_027());
}

#[test]
fn test_stacks_vector_028() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_028());
}

#[test]
fn test_stacks_vector_029() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_029());
}

#[test]
fn test_stacks_vector_030() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_030());
}

#[test]
fn test_stacks_vector_031() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_031());
}

#[test]
fn test_stacks_vector_032() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_032());
}

#[test]
fn test_stacks_vector_033() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_033());
}

#[test]
fn test_stacks_vector_034() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_034());
}

#[test]
fn test_stacks_vector_035() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_035());
}

#[test]
fn test_stacks_vector_036() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_036());
}

#[test]
fn test_stacks_vector_037() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_037());
}

#[test]
fn test_stacks_vector_038() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_038());
}

#[test]
fn test_stacks_vector_039() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_039());
}

#[test]
fn test_stacks_vector_040() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_040());
}

// #[test]
// fn test_stacks_vector_041() {
//     process_stacks_blocks_and_check_expectations(helpers::shapes::get_vector_041());
// }

#[test]
fn test_stacks_vector_042() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_042());
}

#[test]
fn test_stacks_vector_043() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_043());
}

#[test]
fn test_stacks_vector_044() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_044());
}

#[test]
fn test_stacks_vector_045() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_045());
}

#[test]
fn test_stacks_vector_046() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_046());
}

#[test]
fn test_stacks_vector_047() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_047());
}

#[test]
fn test_stacks_vector_048() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_048());
}

#[test]
fn test_stacks_vector_049() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_049());
}

#[test]
fn test_stacks_vector_050() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_050());
}

#[test]
fn test_stacks_vector_051() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_051());
}

#[test]
fn test_stacks_vector_052() {
    process_stacks_blocks_and_check_expectations(helpers::stacks_shapes::get_vector_052());
}

#[test_case(StacksTransactionEvent::STXTransferEvent(STXTransferEventData {
    sender: format!(""),
    recipient: format!(""),
    amount: format!("1"),
}); "stx_transfer")]
#[test_case(StacksTransactionEvent::STXMintEvent(STXMintEventData {
    recipient: format!(""),
    amount: format!("1"),
}); "stx_mint")]
#[test_case(StacksTransactionEvent::STXBurnEvent(STXBurnEventData {
    sender: format!(""),
    amount: format!("1"),
}); "stx_burn")]
#[test_case(StacksTransactionEvent::STXLockEvent(STXLockEventData {
    locked_amount: format!("1"),
    unlock_height: format!(""),
    locked_address: format!(""),
}); "stx_lock")]
#[test_case(StacksTransactionEvent::NFTTransferEvent(NFTTransferEventData {
    asset_class_identifier: format!(""),
    hex_asset_identifier: format!(""),
    sender: format!(""),
    recipient: format!(""),
}); "nft_transfer")]
#[test_case(StacksTransactionEvent::NFTMintEvent(NFTMintEventData {
    asset_class_identifier: format!(""),
    hex_asset_identifier: format!(""),
    recipient: format!(""),
}); "nft_mint")]
#[test_case(StacksTransactionEvent::NFTBurnEvent(NFTBurnEventData {
    asset_class_identifier: format!(""),
    hex_asset_identifier: format!(""),
    sender: format!(""),
}); "nft_burn")]
#[test_case(StacksTransactionEvent::FTTransferEvent(FTTransferEventData {
    asset_class_identifier: format!(""),
    sender: format!(""),
    recipient: format!(""),
    amount: format!("1"),
}); "ft_transfer")]
#[test_case(StacksTransactionEvent::FTMintEvent(FTMintEventData {
    asset_class_identifier: format!(""),
    recipient: format!(""),
    amount: format!("1"),
}); "ft_mint")]
#[test_case(StacksTransactionEvent::FTBurnEvent(FTBurnEventData {
    asset_class_identifier: format!(""),
    sender: format!(""),
    amount: format!("1"),
}); "ft_burn")]
#[test_case(StacksTransactionEvent::DataVarSetEvent(DataVarSetEventData {
    contract_identifier: format!(""),
    var: format!(""),
    hex_new_value: format!(""),
}); "data_var_set")]
#[test_case(StacksTransactionEvent::DataMapInsertEvent(DataMapInsertEventData {
    contract_identifier: format!(""),
    hex_inserted_key: format!(""),
    hex_inserted_value: format!(""),
    map: format!("")
}); "data_map_insert")]
#[test_case(StacksTransactionEvent::DataMapUpdateEvent(DataMapUpdateEventData {
    contract_identifier: format!(""),
    hex_new_value: format!(""),
    hex_key: format!(""),
    map: format!("")
}); "data_map_update")]
#[test_case(StacksTransactionEvent::DataMapDeleteEvent(DataMapDeleteEventData {
    contract_identifier: format!(""),
    hex_deleted_key: format!(""),
    map: format!("")
}); "data_map_delete")]
#[test_case(StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
    contract_identifier: format!(""),
    topic: format!("print"),
    hex_value: format!(""),
}); "smart_contract_print_event")]
fn new_events_can_be_converted_into_chainhook_event(original_event: StacksTransactionEvent) {
    let new_event = create_new_event_from_stacks_event(original_event.clone());
    let event = new_event.into_chainhook_event().unwrap();
    let original_event_serialized = serde_json::to_string(&original_event).unwrap();
    let event_serialized = serde_json::to_string(&event).unwrap();
    assert_eq!(original_event_serialized, event_serialized);
}

#[test]
fn into_chainhook_event_rejects_invalid_missing_event() {
    let new_event = NewEvent {
        txid: format!(""),
        committed: false,
        event_index: 0,
        event_type: format!(""),
        stx_transfer_event: None,
        stx_mint_event: None,
        stx_burn_event: None,
        stx_lock_event: None,
        nft_transfer_event: None,
        nft_mint_event: None,
        nft_burn_event: None,
        ft_transfer_event: None,
        ft_mint_event: None,
        ft_burn_event: None,
        data_var_set_event: None,
        data_map_insert_event: None,
        data_map_update_event: None,
        data_map_delete_event: None,
        contract_event: None,
    };
    new_event
        .into_chainhook_event()
        .expect_err("expected error on missing event");
}
