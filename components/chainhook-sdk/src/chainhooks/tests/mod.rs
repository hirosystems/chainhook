use chainhook_types::{StacksChainEvent, StacksChainUpdatedWithBlocksData, StacksBlockUpdate};
use crate::utils::Context;
use chainhook_types::StacksNetwork;
use crate::chainhooks::types::{StacksPredicate, HookAction};
use super::{stacks::evaluate_stacks_chainhooks_on_chain_event, types::{StacksChainhookSpecification, StacksPrintEventBasedPredicate}};

pub mod fixtures;

#[test]
fn test_stacks_predicate_print_event() {
    // Prepare block
    let event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate {
            block: fixtures::get_stacks_testnet_block(107605).clone(),
            parent_microblocks_to_apply: vec![],
            parent_microblocks_to_rollback: vec![],
        }],
        confirmed_blocks: vec![]
    });

    // Prepare predicate
    let print_predicate = StacksChainhookSpecification {
        uuid: "".to_string(),
        owner_uuid: None,
        name: "".to_string(),
        network: StacksNetwork::Testnet,
        version: 1,
        blocks: None,
        start_block: Some(107604),
        end_block: Some(107607),
        expire_after_occurrence: None,
        capture_all_events: None,
        decode_clarity_values: None,
        predicate: StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
            contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
            contains: "set-loan".to_string(),
        }),
        action: HookAction::Noop,
        enabled: true,
    };

    let predicates = vec![&print_predicate];
    let (triggered, _blocks) = evaluate_stacks_chainhooks_on_chain_event(&event, predicates, &Context::empty());
    assert_eq!(triggered.len(), 1);
}