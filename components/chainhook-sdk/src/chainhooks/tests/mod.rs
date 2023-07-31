use super::{
    stacks::evaluate_stacks_chainhooks_on_chain_event,
    types::{StacksChainhookSpecification, StacksPrintEventBasedPredicate},
};
use crate::chainhooks::types::{HookAction, StacksPredicate};
use crate::utils::Context;
use chainhook_types::StacksNetwork;
use chainhook_types::{StacksBlockUpdate, StacksChainEvent, StacksChainUpdatedWithBlocksData, SmartContractEventData};
use test_case::test_case;

pub mod fixtures;

static PRINT_EVENT_HEX: &str = "0x0c00000002077061796c6f61640c0000000204646174610c0000000d03617072010000000000000000000000000000000c056173736574061ad5d891cb8b4c37b1f6d7d10c093aaeb7c6fad0f00f577261707065642d426974636f696e08626f72726f776572051ac3ff90270cdae87cf86d1b0470e6eeb203f134de0a636f6c6c2d726174696f01000000000000000000000000000000000a636f6c6c2d746f6b656e061ad5d891cb8b4c37b1f6d7d10c093aaeb7c6fad0f00f577261707065642d426974636f696e0a636f6c6c2d7661756c74061ad5d891cb8b4c37b1f6d7d10c093aaeb7c6fad0f00a636f6c6c2d7661756c7407637265617465640100000000000000000000000000252ead0d66756e64696e672d7661756c74061ad5d891cb8b4c37b1f6d7d10c093aaeb7c6fad0f00d66756e64696e672d7661756c740b6c6f616e2d616d6f756e7401000000000000000000000000000017700c6e6578742d7061796d656e7401000000000000000000000000000000000e7061796d656e742d706572696f64010000000000000000000000000000001e1272656d61696e696e672d7061796d656e7473010000000000000000000000000000000c06737461747573020000000101036b6579010000000000000000000000000000000b04747970650d000000087365742d6c6f616e";

static EMPTY_EVENT_HEX: &str = "0x0d00000000";
#[test_case(
    vec![
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            }
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("set-loan".to_string()),
    }),
    1;
    "matches contract_identifier and contains"
)]
#[test_case(
    vec![
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "no-match".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            }
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("set-loan".to_string()),
    }), 
    0;
    "rejects non matching contract_identifier"
)]
#[test_case(
    vec![
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            }
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("set-loan".to_string()),
    }), 
    0;
    "rejects non matching contains value"
)]
#[test_case(
    vec![
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            }
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: None,
        contains: Some("set-loan".to_string()),
    }), 
    1;
    "ommitting contract_identifier checks all print events for match"
)]
#[test_case(
    vec![
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            }
        ],
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            }
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: None,
    }), 
    1;
    "ommitting contains matches all values for matching events"
)]
#[test_case(
    vec![
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            }
        ],
        vec![
            SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            }
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: None,
        contains: None,
    }), 
    2;
    "ommitting contract_identifier and contains matches all values on all print events"
)]

fn test_stacks_predicate_smart_contract_event(blocks_with_events: Vec<Vec<SmartContractEventData>>, predicate_event: StacksPredicate, expected_applies: u64) {
    // Prepare block
    let new_blocks = blocks_with_events.iter().map(|events| StacksBlockUpdate {
        block: fixtures::build_stacks_testnet_block_from_smart_contract_event_data(events),
        parent_microblocks_to_apply: vec![],
        parent_microblocks_to_rollback: vec![],
    }).collect();
    let event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks,
        confirmed_blocks: vec![],
    });
    // Prepare predicate
    let print_predicate = StacksChainhookSpecification {
        uuid: "".to_string(),
        owner_uuid: None,
        name: "".to_string(),
        network: StacksNetwork::Testnet,
        version: 1,
        blocks: None,
        start_block: None,
        end_block: None,
        expire_after_occurrence: None,
        capture_all_events: None,
        decode_clarity_values: None,
        predicate: predicate_event,
        action: HookAction::Noop,
        enabled: true,
    };

    let predicates = vec![&print_predicate];
    let (triggered, _blocks) =
        evaluate_stacks_chainhooks_on_chain_event(&event, predicates, &Context::empty());

    if expected_applies == 0 {
        assert_eq!(triggered.len(), 0)
    }
    else {
        let actual_applies: u64 = triggered[0].apply.len().try_into().unwrap();
        assert_eq!(actual_applies, expected_applies);
    }
}
