use super::{
    stacks::evaluate_stacks_chainhooks_on_chain_event,
    types::{StacksChainhookSpecification, StacksPrintEventBasedPredicate, StacksNftEventBasedPredicate, StacksFtEventBasedPredicate,StacksContractCallBasedPredicate,StacksContractDeploymentPredicate, ExactMatchingRule},
};
use crate::chainhooks::types::{HookAction, StacksPredicate, StacksStxEventBasedPredicate,};
use crate::utils::Context;
use chainhook_types::{StacksNetwork, StacksTransactionEvent,STXTransferEventData,STXMintEventData,STXLockEventData, FTMintEventData,FTTransferEventData,FTBurnEventData,NFTMintEventData,NFTBurnEventData,NFTTransferEventData, STXBurnEventData};
use chainhook_types::{StacksBlockUpdate, StacksChainEvent, StacksChainUpdatedWithBlocksData, SmartContractEventData};
use test_case::test_case;

pub mod fixtures;

static PRINT_EVENT_HEX: &str = "0x0d00000010616263736f6d652d76616c7565616263"; // "abcsome-valueabc"

static EMPTY_EVENT_HEX: &str = "0x0d00000000";

// FtEvent predicate tests
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTMintEvent(FTMintEventData {
                asset_class_identifier: "asset-id".to_string(),
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string()]
    }),
    1;
    "FtEvent predicates match mint event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTTransferEvent(FTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "asset-id".to_string(),
                amount: "".to_string(),
                recipient: "".to_string() 
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["transfer".to_string()]
    }),
    1;
    "FtEvent predicates match transfer event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTBurnEvent(FTBurnEventData {
                asset_class_identifier: "asset-id".to_string(),
                sender: "".to_string(),
                amount: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["burn".to_string()]
    }),
    1;
    "FtEvent predicates match burn event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTMintEvent(FTMintEventData {
                asset_class_identifier: "not-asset-id".to_string(),
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string()]
    }),
    0 => ignore;
    "FtEvent predicates reject no-match asset id for mint event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTTransferEvent(FTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "not-asset-id".to_string(),
                amount: "".to_string(),
                recipient: "".to_string() 
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["transfer".to_string()]
    }),
    0 => ignore;
    "FtEvent predicates reject no-match asset id for transfer event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTBurnEvent(FTBurnEventData {
                asset_class_identifier: "not-asset-id".to_string(),
                sender: "".to_string(),
                amount: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["burn".to_string()]
    }),
    0 => ignore;
    "FtEvent predicates reject no-match asset id for burn event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTMintEvent(FTMintEventData {
                asset_class_identifier: "asset-id".to_string(),
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ],
        vec![
            StacksTransactionEvent::FTTransferEvent(FTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "asset-id".to_string(),
                amount: "".to_string(),
                recipient: "".to_string() 
            })
        ],
        vec![
            StacksTransactionEvent::FTBurnEvent(FTBurnEventData {
                asset_class_identifier: "asset-id".to_string(),
                sender: "".to_string(),
                amount: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string(),"transfer".to_string(), "burn".to_string()]
    }),
    3;
    "FtEvent predicates match multiple events"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::FTTransferEvent(FTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "asset-id".to_string(),
                amount: "".to_string(),
                recipient: "".to_string() 
            })
        ],
        vec![
            StacksTransactionEvent::FTBurnEvent(FTBurnEventData {
                asset_class_identifier: "asset-id".to_string(),
                sender: "".to_string(),
                amount: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::FtEvent(StacksFtEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string()]
    }),
    0;
    "FtEvent predicates don't match if missing event"
)]

// NftEvent predicate tests
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::NFTMintEvent(NFTMintEventData {
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                recipient: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::NftEvent(StacksNftEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string()]
    }),
    1;
    "NftEvent predicates match mint event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::NFTTransferEvent(NFTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                recipient: "".to_string() 
            })
        ]
    ], 
    StacksPredicate::NftEvent(StacksNftEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["transfer".to_string()]
    }),
    1;
    "NftEvent predicates match transfer event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::NFTBurnEvent(NFTBurnEventData {
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                sender: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::NftEvent(StacksNftEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["burn".to_string()]
    }),
    1;
    "NftEvent predicates match burn event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::NFTMintEvent(NFTMintEventData {
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                recipient: "".to_string(),
            })
        ],
        vec![
            StacksTransactionEvent::NFTTransferEvent(NFTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                recipient: "".to_string() 
            })
        ],
        vec![
            StacksTransactionEvent::NFTBurnEvent(NFTBurnEventData {
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                sender: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::NftEvent(StacksNftEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string(),"transfer".to_string(), "burn".to_string()]
    }),
    3;
    "NftEvent predicates match multiple events"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::NFTTransferEvent(NFTTransferEventData { 
                sender: "".to_string(),
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                recipient: "".to_string() 
            })
        ],
        vec![
            StacksTransactionEvent::NFTBurnEvent(NFTBurnEventData {
                asset_class_identifier: "asset-id".to_string(),
                hex_asset_identifier: "asset-id".to_string(),
                sender: "".to_string(),
            })
        ]
    ], 
    StacksPredicate::NftEvent(StacksNftEventBasedPredicate {
        asset_identifier: "asset-id".to_string(),
        actions: vec!["mint".to_string()]
    }),
    0;
    "NftEvent predicates don't match if missing event"
)]
// StxEvent predicate tests
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::STXMintEvent(STXMintEventData {
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ]
    ], 
    StacksPredicate::StxEvent(StacksStxEventBasedPredicate {
        actions: vec!["mint".to_string()]
    }),
    1;
    "StxEvent predicates match mint event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::STXTransferEvent(STXTransferEventData {
                sender: "".to_string(),
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ]
    ], 
    StacksPredicate::StxEvent(StacksStxEventBasedPredicate {
        actions: vec!["transfer".to_string()]
    }),
    1;
    "StxEvent predicates match transfer event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::STXLockEvent(STXLockEventData {
                locked_amount: "".to_string(),
                unlock_height: "".to_string(),
                locked_address: "".to_string()
            })
        ]
    ], 
    StacksPredicate::StxEvent(StacksStxEventBasedPredicate {
        actions: vec!["lock".to_string()]
    }),
    1;
    "StxEvent predicates match lock event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::STXBurnEvent(STXBurnEventData {
                sender: "".to_string(),
                amount: "".to_string()
            })
        ]
    ], 
    StacksPredicate::StxEvent(StacksStxEventBasedPredicate {
        actions: vec!["burn".to_string()]
    }),
    1 => ignore;
    "StxEvent predicates match burn event"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::STXMintEvent(STXMintEventData {
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ],
        vec![
            StacksTransactionEvent::STXTransferEvent(STXTransferEventData {
                sender: "".to_string(),
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ],
        vec![
            StacksTransactionEvent::STXLockEvent(STXLockEventData {
                locked_amount: "".to_string(),
                unlock_height: "".to_string(),
                locked_address: "".to_string()
            })
        ]
    ], 
    StacksPredicate::StxEvent(StacksStxEventBasedPredicate {
        actions: vec!["mint".to_string(), "transfer".to_string(), "lock".to_string()]
    }),
    3;
    "StxEvent predicates match multiple events"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::STXTransferEvent(STXTransferEventData {
                sender: "".to_string(),
                recipient: "".to_string(),
                amount: "".to_string()
            })
        ],
        vec![
            StacksTransactionEvent::STXLockEvent(STXLockEventData {
                locked_amount: "".to_string(),
                unlock_height: "".to_string(),
                locked_address: "".to_string()
            })
        ]
    ], 
    StacksPredicate::StxEvent(StacksStxEventBasedPredicate {
        actions: vec!["mint".to_string()]
    }),
    0;
    "StxEvent predicates don't match if missing event"
)]

// PrintEvent predicate tests
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("some-value".to_string()),
    }),
    1;
    "PrintEvent predicate matches contract_identifier and contains"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "not-print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("some-value".to_string()),
    }),
    0;
    "PrintEvent predicate does not check events with topic other than print"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "no-match".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("some-value".to_string()),
    }), 
    0;
    "PrintEvent predicate rejects non matching contract_identifier"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: Some("some-value".to_string()),
    }), 
    0;
    "PrintEvent predicate rejects non matching contains value"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: None,
        contains: Some("some-value".to_string()),
    }), 
    1;
    "PrintEvent predicate ommitting contract_identifier checks all print events for match"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            })
        ],
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: Some(
            "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
        ),
        contains: None,
    }), 
    1;
    "PrintEvent predicate ommitting contains matches all values for matching events"
)]
#[test_case(
    vec![
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "ST3AXH4EBHD63FCFPTZ8GR29TNTVWDYPGY0KDY5E5.loan-data".to_string(),
                hex_value: EMPTY_EVENT_HEX.to_string()
            })
        ],
        vec![
            StacksTransactionEvent::SmartContractEvent(SmartContractEventData {
                topic: "print".to_string(),
                contract_identifier: "".to_string(),
                hex_value: PRINT_EVENT_HEX.to_string()
            })
        ]
    ], 
    StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate {
        contract_identifier: None,
        contains: None,
    }), 
    2;
    "PrintEvent predicate ommitting contract_identifier and contains matches all values on all print events"
)]

fn test_stacks_predicate_events(blocks_with_events: Vec<Vec<StacksTransactionEvent>>, predicate_event: StacksPredicate, expected_applies: u64) {
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


#[test_case(
    StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer("ST13F481SBR0R7Z6NMMH8YV2FJJYXA5JPA0AD3HP9".to_string())), 
    1;
    "Deployer predicate matches by contract deployer"
)]
#[test_case(
    StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer("*".to_string())), 
    1;
    "Deployer predicate matches with wildcard deployer"
)]
#[test_case(
    StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer("deployer".to_string())), 
    0;
    "Deployer predicate does not match non-matching deployer"
)]
#[test_case(
    StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::ImplementSip09), 
    0;
    "ImplementSip09 predicate returns no values"
)]
#[test_case(
    StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::ImplementSip10), 
    0;
    "ImplementSip10 predicate returns no values"
)]
fn test_stacks_predicate_contract_deploy(predicate_event: StacksPredicate, expected_applies: u64) {
    // Prepare block
    let new_blocks = vec![StacksBlockUpdate {
        block: fixtures::build_stacks_testnet_block_with_contract_deployment(),
        parent_microblocks_to_apply: vec![],
        parent_microblocks_to_rollback: vec![],
    }, StacksBlockUpdate {
        block: fixtures::build_stacks_testnet_block_with_contract_call(),
        parent_microblocks_to_apply: vec![],
        parent_microblocks_to_rollback: vec![],
    }];
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
    else if triggered.len() == 0 {
        panic!("expected more than one block to be applied, but no predicates were triggered")
    }
    else {
        let actual_applies: u64 = triggered[0].apply.len().try_into().unwrap();
        assert_eq!(actual_applies, expected_applies);
    }
}

#[test_case(
    StacksPredicate::ContractCall(StacksContractCallBasedPredicate {
        contract_identifier: "ST13F481SBR0R7Z6NMMH8YV2FJJYXA5JPA0AD3HP9.subnet-v1".to_string(),
        method: "commit-block".to_string()
    }), 
    1;
    "ContractCall predicate matches by contract identifier and method"
)]
#[test_case(
    StacksPredicate::ContractCall(StacksContractCallBasedPredicate {
        contract_identifier: "ST13F481SBR0R7Z6NMMH8YV2FJJYXA5JPA0AD3HP9.subnet-v1".to_string(),
        method: "wrong-method".to_string()
    }), 
    0;
    "ContractCall predicate does not match for wrong method"
)]
#[test_case(
    StacksPredicate::ContractCall(StacksContractCallBasedPredicate {
        contract_identifier: "wrong-id".to_string(),
        method: "commit-block".to_string()
    }), 
    0;
    "ContractCall predicate does not match for wrong contract identifier"
)]
#[test_case(
    StacksPredicate::Txid(ExactMatchingRule::Equals("0xb92c2ade84a8b85f4c72170680ae42e65438aea4db72ba4b2d6a6960f4141ce8".to_string())), 
    1;
    "Txid predicate matches by a transaction's id"
)]
#[test_case(
    StacksPredicate::Txid(ExactMatchingRule::Equals("wrong-id".to_string())), 
    0;
    "Txid predicate rejects non matching id"
)]
fn test_stacks_predicate_contract_call(predicate_event: StacksPredicate, expected_applies: u64) {
    // Prepare block
    let new_blocks = vec![StacksBlockUpdate {
        block: fixtures::build_stacks_testnet_block_with_contract_call(),
        parent_microblocks_to_apply: vec![],
        parent_microblocks_to_rollback: vec![],
    },StacksBlockUpdate {
        block: fixtures::build_stacks_testnet_block_with_contract_deployment(),
        parent_microblocks_to_apply: vec![],
        parent_microblocks_to_rollback: vec![],
    }];
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
    else if triggered.len() == 0 {
        panic!("expected more than one block to be applied, but no predicates were triggered")
    }
    else {
        let actual_applies: u64 = triggered[0].apply.len().try_into().unwrap();
        assert_eq!(actual_applies, expected_applies);
    }
}