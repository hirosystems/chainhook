use crate::utils::{AbstractStacksBlock, Context};

use super::types::{
    BlockIdentifierIndexRule, ExactMatchingRule, HookAction, StacksChainhookSpecification,
    StacksContractDeploymentPredicate, StacksPredicate, StacksPrintEventBasedPredicate,
};
use chainhook_types::{
    BlockIdentifier, StacksChainEvent, StacksTransactionData, StacksTransactionEvent,
    StacksTransactionKind, TransactionIdentifier,
};
use hiro_system_kit::slog;
use regex::Regex;
use reqwest::{Client, Method};
use serde_json::Value as JsonValue;
use stacks_rpc_client::clarity::stacks_common::codec::StacksMessageCodec;
use stacks_rpc_client::clarity::vm::types::{CharType, SequenceData, Value as ClarityValue};
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use reqwest::RequestBuilder;

pub struct StacksTriggerChainhook<'a> {
    pub chainhook: &'a StacksChainhookSpecification,
    pub apply: Vec<(Vec<&'a StacksTransactionData>, &'a dyn AbstractStacksBlock)>,
    pub rollback: Vec<(Vec<&'a StacksTransactionData>, &'a dyn AbstractStacksBlock)>,
}

#[derive(Clone, Debug)]
pub struct StacksApplyTransactionPayload {
    pub block_identifier: BlockIdentifier,
    pub transactions: Vec<StacksTransactionData>,
}

#[derive(Clone, Debug)]
pub struct StacksRollbackTransactionPayload {
    pub block_identifier: BlockIdentifier,
    pub transactions: Vec<StacksTransactionData>,
}

#[derive(Clone, Debug)]
pub struct StacksChainhookPayload {
    pub uuid: String,
}

#[derive(Clone, Debug)]
pub struct StacksChainhookOccurrencePayload {
    pub apply: Vec<StacksApplyTransactionPayload>,
    pub rollback: Vec<StacksRollbackTransactionPayload>,
    pub chainhook: StacksChainhookPayload,
}
pub enum StacksChainhookOccurrence {
    Http(RequestBuilder),
    File(String, Vec<u8>),
    Data(StacksChainhookOccurrencePayload),
}

impl<'a> StacksTriggerChainhook<'a> {
    pub fn should_decode_clarity_value(&self) -> bool {
        self.chainhook.decode_clarity_values.unwrap_or(false)
    }
}

pub fn evaluate_stacks_chainhooks_on_chain_event<'a>(
    chain_event: &'a StacksChainEvent,
    active_chainhooks: Vec<&'a StacksChainhookSpecification>,
    ctx: &Context,
) -> (
    Vec<StacksTriggerChainhook<'a>>,
    BTreeMap<&'a str, &'a BlockIdentifier>,
    BTreeMap<&'a str, &'a BlockIdentifier>,
) {
    let mut triggered_predicates = vec![];
    let mut evaluated_predicates = BTreeMap::new();
    let mut expired_predicates = BTreeMap::new();
    match chain_event {
        StacksChainEvent::ChainUpdatedWithBlocks(update) => {
            for chainhook in active_chainhooks.iter() {
                let mut apply = vec![];
                let mut rollback = vec![];
                for block_update in update.new_blocks.iter() {
                    evaluated_predicates.insert(
                        chainhook.uuid.as_str(),
                        &block_update.block.block_identifier,
                    );

                    for parents_microblock_to_apply in
                        block_update.parent_microblocks_to_apply.iter()
                    {
                        let (mut occurrences, mut expirations) =
                            evaluate_stacks_chainhook_on_blocks(
                                vec![parents_microblock_to_apply],
                                chainhook,
                                ctx,
                            );
                        apply.append(&mut occurrences);
                        expired_predicates.append(&mut expirations);
                    }
                    for parents_microblock_to_rolllback in
                        block_update.parent_microblocks_to_rollback.iter()
                    {
                        let (mut occurrences, mut expirations) =
                            evaluate_stacks_chainhook_on_blocks(
                                vec![parents_microblock_to_rolllback],
                                chainhook,
                                ctx,
                            );
                        rollback.append(&mut occurrences);
                        expired_predicates.append(&mut expirations);
                    }

                    let (mut occurrences, mut expirations) = evaluate_stacks_chainhook_on_blocks(
                        vec![&block_update.block],
                        chainhook,
                        ctx,
                    );
                    apply.append(&mut occurrences);
                    expired_predicates.append(&mut expirations);
                }
                if !apply.is_empty() || !rollback.is_empty() {
                    triggered_predicates.push(StacksTriggerChainhook {
                        chainhook,
                        apply,
                        rollback,
                    })
                }
            }
        }
        StacksChainEvent::ChainUpdatedWithMicroblocks(update) => {
            for chainhook in active_chainhooks.iter() {
                let mut apply = vec![];
                let rollback = vec![];

                for microblock_to_apply in update.new_microblocks.iter() {
                    evaluated_predicates.insert(
                        chainhook.uuid.as_str(),
                        &microblock_to_apply.metadata.anchor_block_identifier,
                    );

                    let (mut occurrences, mut expirations) = evaluate_stacks_chainhook_on_blocks(
                        vec![microblock_to_apply],
                        chainhook,
                        ctx,
                    );
                    apply.append(&mut occurrences);
                    expired_predicates.append(&mut expirations);
                }
                if !apply.is_empty() || !rollback.is_empty() {
                    triggered_predicates.push(StacksTriggerChainhook {
                        chainhook,
                        apply,
                        rollback,
                    })
                }
            }
        }
        StacksChainEvent::ChainUpdatedWithMicroblocksReorg(update) => {
            for chainhook in active_chainhooks.iter() {
                let mut apply = vec![];
                let mut rollback = vec![];

                for microblock_to_apply in update.microblocks_to_apply.iter() {
                    evaluated_predicates.insert(
                        chainhook.uuid.as_str(),
                        &microblock_to_apply.metadata.anchor_block_identifier,
                    );
                    let (mut occurrences, mut expirations) = evaluate_stacks_chainhook_on_blocks(
                        vec![microblock_to_apply],
                        chainhook,
                        ctx,
                    );
                    apply.append(&mut occurrences);
                    expired_predicates.append(&mut expirations);
                }
                for microblock_to_rollback in update.microblocks_to_rollback.iter() {
                    let (mut occurrences, mut expirations) = evaluate_stacks_chainhook_on_blocks(
                        vec![microblock_to_rollback],
                        chainhook,
                        ctx,
                    );
                    rollback.append(&mut occurrences);
                    expired_predicates.append(&mut expirations);
                }
                if !apply.is_empty() || !rollback.is_empty() {
                    triggered_predicates.push(StacksTriggerChainhook {
                        chainhook,
                        apply,
                        rollback,
                    })
                }
            }
        }
        StacksChainEvent::ChainUpdatedWithReorg(update) => {
            for chainhook in active_chainhooks.iter() {
                let mut apply = vec![];
                let mut rollback = vec![];

                for block_update in update.blocks_to_apply.iter() {
                    evaluated_predicates.insert(
                        chainhook.uuid.as_str(),
                        &block_update.block.block_identifier,
                    );
                    for parents_microblock_to_apply in
                        block_update.parent_microblocks_to_apply.iter()
                    {
                        let (mut occurrences, mut expirations) =
                            evaluate_stacks_chainhook_on_blocks(
                                vec![parents_microblock_to_apply],
                                chainhook,
                                ctx,
                            );
                        apply.append(&mut occurrences);
                        expired_predicates.append(&mut expirations);
                    }

                    let (mut occurrences, mut expirations) = evaluate_stacks_chainhook_on_blocks(
                        vec![&block_update.block],
                        chainhook,
                        ctx,
                    );
                    apply.append(&mut occurrences);
                    expired_predicates.append(&mut expirations);
                }
                for block_update in update.blocks_to_rollback.iter() {
                    for parents_microblock_to_rollback in
                        block_update.parent_microblocks_to_rollback.iter()
                    {
                        let (mut occurrences, mut expirations) =
                            evaluate_stacks_chainhook_on_blocks(
                                vec![parents_microblock_to_rollback],
                                chainhook,
                                ctx,
                            );
                        rollback.append(&mut occurrences);
                        expired_predicates.append(&mut expirations);
                    }
                    let (mut occurrences, mut expirations) = evaluate_stacks_chainhook_on_blocks(
                        vec![&block_update.block],
                        chainhook,
                        ctx,
                    );
                    rollback.append(&mut occurrences);
                    expired_predicates.append(&mut expirations);
                }
                if !apply.is_empty() || !rollback.is_empty() {
                    triggered_predicates.push(StacksTriggerChainhook {
                        chainhook,
                        apply,
                        rollback,
                    })
                }
            }
        }
    }
    (
        triggered_predicates,
        evaluated_predicates,
        expired_predicates,
    )
}

pub fn evaluate_stacks_chainhook_on_blocks<'a>(
    blocks: Vec<&'a dyn AbstractStacksBlock>,
    chainhook: &'a StacksChainhookSpecification,
    ctx: &Context,
) -> (
    Vec<(Vec<&'a StacksTransactionData>, &'a dyn AbstractStacksBlock)>,
    BTreeMap<&'a str, &'a BlockIdentifier>,
) {
    let mut occurrences = vec![];
    let mut expired_predicates = BTreeMap::new();
    let end_block = chainhook.end_block.unwrap_or(u64::MAX);
    for block in blocks {
        if end_block >= block.get_identifier().index {
            let mut hits = vec![];
            if chainhook.is_predicate_targeting_block_header() {
                if evaluate_stacks_predicate_on_block(block, chainhook, ctx) {
                    for tx in block.get_transactions().iter() {
                        hits.push(tx);
                    }
                }
            } else {
                for tx in block.get_transactions().iter() {
                    if evaluate_stacks_predicate_on_transaction(tx, chainhook, ctx) {
                        hits.push(tx);
                    }
                }
            }
            if hits.len() > 0 {
                occurrences.push((hits, block));
            }
        } else {
            expired_predicates.insert(chainhook.uuid.as_str(), block.get_identifier());
        }
    }
    (occurrences, expired_predicates)
}

pub fn evaluate_stacks_predicate_on_block<'a>(
    block: &'a dyn AbstractStacksBlock,
    chainhook: &'a StacksChainhookSpecification,
    _ctx: &Context,
) -> bool {
    match &chainhook.predicate {
        StacksPredicate::BlockHeight(BlockIdentifierIndexRule::Between(a, b)) => {
            block.get_identifier().index.gt(a) && block.get_identifier().index.lt(b)
        }
        StacksPredicate::BlockHeight(BlockIdentifierIndexRule::HigherThan(a)) => {
            block.get_identifier().index.gt(a)
        }
        StacksPredicate::BlockHeight(BlockIdentifierIndexRule::LowerThan(a)) => {
            block.get_identifier().index.lt(a)
        }
        StacksPredicate::BlockHeight(BlockIdentifierIndexRule::Equals(a)) => {
            block.get_identifier().index.eq(a)
        }
        StacksPredicate::ContractDeployment(_)
        | StacksPredicate::ContractCall(_)
        | StacksPredicate::FtEvent(_)
        | StacksPredicate::NftEvent(_)
        | StacksPredicate::StxEvent(_)
        | StacksPredicate::PrintEvent(_)
        | StacksPredicate::Txid(_) => unreachable!(),
    }
}

pub fn evaluate_stacks_predicate_on_transaction<'a>(
    transaction: &'a StacksTransactionData,
    chainhook: &'a StacksChainhookSpecification,
    ctx: &Context,
) -> bool {
    match &chainhook.predicate {
        StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::Deployer(
            expected_deployer,
        )) => match &transaction.metadata.kind {
            StacksTransactionKind::ContractDeployment(actual_deployment) => {
                if expected_deployer.eq("*") {
                    true
                } else {
                    actual_deployment
                        .contract_identifier
                        .starts_with(expected_deployer)
                }
            }
            _ => false,
        },
        StacksPredicate::ContractDeployment(StacksContractDeploymentPredicate::ImplementTrait(
            stacks_trait,
        )) => match stacks_trait {
            _ => match &transaction.metadata.kind {
                StacksTransactionKind::ContractDeployment(_actual_deployment) => {
                    ctx.try_log(|logger| {
                        slog::warn!(
                            logger,
                            "StacksContractDeploymentPredicate::ImplementTrait uninmplemented"
                        )
                    });
                    false
                }
                _ => false,
            },
        },
        StacksPredicate::ContractCall(expected_contract_call) => match &transaction.metadata.kind {
            StacksTransactionKind::ContractCall(actual_contract_call) => {
                actual_contract_call
                    .contract_identifier
                    .eq(&expected_contract_call.contract_identifier)
                    && actual_contract_call
                        .method
                        .eq(&expected_contract_call.method)
            }
            _ => false,
        },
        StacksPredicate::FtEvent(expected_event) => {
            let expecting_mint = expected_event.actions.contains(&"mint".to_string());
            let expecting_transfer = expected_event.actions.contains(&"transfer".to_string());
            let expecting_burn = expected_event.actions.contains(&"burn".to_string());

            for event in transaction.metadata.receipt.events.iter() {
                match (event, expecting_mint, expecting_transfer, expecting_burn) {
                    (StacksTransactionEvent::FTMintEvent(ft_event), true, _, _) => {
                        if ft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEvent::FTTransferEvent(ft_event), _, true, _) => {
                        if ft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEvent::FTBurnEvent(ft_event), _, _, true) => {
                        if ft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    _ => continue,
                }
            }
            false
        }
        StacksPredicate::NftEvent(expected_event) => {
            let expecting_mint = expected_event.actions.contains(&"mint".to_string());
            let expecting_transfer = expected_event.actions.contains(&"transfer".to_string());
            let expecting_burn = expected_event.actions.contains(&"burn".to_string());

            for event in transaction.metadata.receipt.events.iter() {
                match (event, expecting_mint, expecting_transfer, expecting_burn) {
                    (StacksTransactionEvent::NFTMintEvent(nft_event), true, _, _) => {
                        if nft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEvent::NFTTransferEvent(nft_event), _, true, _) => {
                        if nft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEvent::NFTBurnEvent(nft_event), _, _, true) => {
                        if nft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    _ => continue,
                }
            }
            false
        }
        StacksPredicate::StxEvent(expected_event) => {
            let expecting_mint = expected_event.actions.contains(&"mint".to_string());
            let expecting_transfer = expected_event.actions.contains(&"transfer".to_string());
            let expecting_lock = expected_event.actions.contains(&"lock".to_string());
            let expecting_burn = expected_event.actions.contains(&"burn".to_string());

            for event in transaction.metadata.receipt.events.iter() {
                match (
                    event,
                    expecting_mint,
                    expecting_transfer,
                    expecting_lock,
                    expecting_burn,
                ) {
                    (StacksTransactionEvent::STXMintEvent(_), true, _, _, _) => return true,
                    (StacksTransactionEvent::STXTransferEvent(_), _, true, _, _) => return true,
                    (StacksTransactionEvent::STXLockEvent(_), _, _, true, _) => return true,
                    (StacksTransactionEvent::STXBurnEvent(_), _, _, _, true) => return true,
                    _ => continue,
                }
            }
            false
        }
        StacksPredicate::PrintEvent(expected_event) => {
            for event in transaction.metadata.receipt.events.iter() {
                match event {
                    StacksTransactionEvent::SmartContractEvent(actual) => {
                        if actual.topic == "print" {
                            match expected_event {
                                StacksPrintEventBasedPredicate::Contains {
                                    contract_identifier,
                                    contains,
                                } => {
                                    if contract_identifier == &actual.contract_identifier
                                        || contract_identifier == "*"
                                    {
                                        if contains == "*" {
                                            return true;
                                        }
                                        let value = format!(
                                            "{}",
                                            expect_decoded_clarity_value(&actual.hex_value)
                                        );
                                        if value.contains(contains) {
                                            return true;
                                        }
                                    }
                                }
                                StacksPrintEventBasedPredicate::MatchesRegex {
                                    contract_identifier,
                                    regex,
                                } => {
                                    if contract_identifier == &actual.contract_identifier
                                        || contract_identifier == "*"
                                    {
                                        if let Ok(regex) = Regex::new(regex) {
                                            let value = format!(
                                                "{}",
                                                expect_decoded_clarity_value(&actual.hex_value)
                                            );
                                            if regex.is_match(&value) {
                                                return true;
                                            }
                                        } else {
                                            ctx.try_log(|logger| {
                                                slog::error!(logger, "unable to parse print_event matching rule as regex")
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        }
        StacksPredicate::Txid(ExactMatchingRule::Equals(txid)) => {
            txid.eq(&transaction.transaction_identifier.hash)
        }
        StacksPredicate::BlockHeight(_) => unreachable!(),
    }
}

fn serialize_stacks_block(
    block: &dyn AbstractStacksBlock,
    transactions: Vec<&StacksTransactionData>,
    decode_clarity_values: bool,
    include_contract_abi: bool,
    ctx: &Context,
) -> serde_json::Value {
    json!({
        "block_identifier": block.get_identifier(),
        "parent_block_identifier": block.get_parent_identifier(),
        "timestamp": block.get_timestamp(),
        "transactions": transactions.into_iter().map(|transaction| {
            serialize_stacks_transaction(&transaction, decode_clarity_values, include_contract_abi, ctx)
        }).collect::<Vec<_>>(),
        "metadata": block.get_serialized_metadata(),
    })
}

fn serialize_stacks_transaction(
    transaction: &StacksTransactionData,
    decode_clarity_values: bool,
    include_contract_abi: bool,
    ctx: &Context,
) -> serde_json::Value {
    let mut json = json!({
        "transaction_identifier": transaction.transaction_identifier,
        "operations": transaction.operations,
        "metadata": {
            "success": transaction.metadata.success,
            "raw_tx": transaction.metadata.raw_tx,
            "result": if decode_clarity_values {
                serialized_decoded_clarity_value(&transaction.metadata.result, ctx)
            } else  {
                json!(transaction.metadata.result)
            },
            "sender": transaction.metadata.sender,
            "nonce": transaction.metadata.nonce,
            "fee": transaction.metadata.fee,
            "kind": transaction.metadata.kind,
            "receipt": {
                "mutated_contracts_radius": transaction.metadata.receipt.mutated_contracts_radius,
                "mutated_assets_radius": transaction.metadata.receipt.mutated_assets_radius,
                "contract_calls_stack": transaction.metadata.receipt.contract_calls_stack,
                "events": transaction.metadata.receipt.events.iter().map(|event| {
                    if decode_clarity_values { serialized_event_with_decoded_clarity_value(event, ctx) } else { json!(event) }
                }).collect::<Vec<serde_json::Value>>(),
            },
            "description": transaction.metadata.description,
            "sponsor": transaction.metadata.sponsor,
            "execution_cost": transaction.metadata.execution_cost,
            "position": transaction.metadata.position
        },
    });
    if include_contract_abi {
        if let Some(abi) = &transaction.metadata.contract_abi {
            json["metadata"]["contract_abi"] = json!(abi);
        }
    }
    json
}

pub fn serialized_event_with_decoded_clarity_value(
    event: &StacksTransactionEvent,
    ctx: &Context,
) -> serde_json::Value {
    match event {
        StacksTransactionEvent::STXTransferEvent(payload) => {
            json!({
                "type": "STXTransferEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::STXMintEvent(payload) => {
            json!({
                "type": "STXMintEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::STXLockEvent(payload) => {
            json!({
                "type": "STXLockEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::STXBurnEvent(payload) => {
            json!({
                "type": "STXBurnEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::NFTTransferEvent(payload) => {
            json!({
                "type": "NFTTransferEvent",
                "data": {
                    "asset_class_identifier": payload.asset_class_identifier,
                    "asset_identifier": serialized_decoded_clarity_value(&payload.hex_asset_identifier, ctx),
                    "sender": payload.sender,
                    "recipient": payload.recipient,
                }
            })
        }
        StacksTransactionEvent::NFTMintEvent(payload) => {
            json!({
                "type": "NFTMintEvent",
                "data": {
                    "asset_class_identifier": payload.asset_class_identifier,
                    "asset_identifier": serialized_decoded_clarity_value(&payload.hex_asset_identifier, ctx),
                    "recipient": payload.recipient,
                }
            })
        }
        StacksTransactionEvent::NFTBurnEvent(payload) => {
            json!({
                "type": "NFTBurnEvent",
                "data": {
                    "asset_class_identifier": payload.asset_class_identifier,
                    "asset_identifier": serialized_decoded_clarity_value(&payload.hex_asset_identifier, ctx),
                    "sender": payload.sender,
                }
            })
        }
        StacksTransactionEvent::FTTransferEvent(payload) => {
            json!({
                "type": "FTTransferEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::FTMintEvent(payload) => {
            json!({
                "type": "FTMintEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::FTBurnEvent(payload) => {
            json!({
                "type": "FTBurnEvent",
                "data": payload
            })
        }
        StacksTransactionEvent::DataVarSetEvent(payload) => {
            json!({
                "type": "DataVarSetEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "var": payload.var,
                    "new_value": serialized_decoded_clarity_value(&payload.hex_new_value, ctx),
                }
            })
        }
        StacksTransactionEvent::DataMapInsertEvent(payload) => {
            json!({
                "type": "DataMapInsertEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "map": payload.map,
                    "inserted_key": serialized_decoded_clarity_value(&payload.hex_inserted_key, ctx),
                    "inserted_value": serialized_decoded_clarity_value(&payload.hex_inserted_value, ctx),
                }
            })
        }
        StacksTransactionEvent::DataMapUpdateEvent(payload) => {
            json!({
                "type": "DataMapUpdateEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "map": payload.map,
                    "key": serialized_decoded_clarity_value(&payload.hex_key, ctx),
                    "new_value": serialized_decoded_clarity_value(&payload.hex_new_value, ctx),
                }
            })
        }
        StacksTransactionEvent::DataMapDeleteEvent(payload) => {
            json!({
                "type": "DataMapDeleteEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "map": payload.map,
                    "deleted_key": serialized_decoded_clarity_value(&payload.hex_deleted_key, ctx),
                }
            })
        }
        StacksTransactionEvent::SmartContractEvent(payload) => {
            json!({
                "type": "SmartContractEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "topic": payload.topic,
                    "value": serialized_decoded_clarity_value(&payload.hex_value, ctx),
                }
            })
        }
    }
}

pub fn expect_decoded_clarity_value(hex_value: &str) -> ClarityValue {
    try_decode_clarity_value(hex_value)
        .expect("unable to decode clarity value emitted by stacks-node")
}

pub fn try_decode_clarity_value(hex_value: &str) -> Option<ClarityValue> {
    let hex_value = hex_value.strip_prefix("0x")?;
    let value_bytes = hex::decode(&hex_value).ok()?;
    ClarityValue::consensus_deserialize(&mut Cursor::new(&value_bytes)).ok()
}

pub fn serialized_decoded_clarity_value(hex_value: &str, ctx: &Context) -> serde_json::Value {
    let hex_value = match hex_value.strip_prefix("0x") {
        Some(hex_value) => hex_value,
        _ => return json!(hex_value.to_string()),
    };
    let value_bytes = match hex::decode(&hex_value) {
        Ok(bytes) => bytes,
        _ => return json!(hex_value.to_string()),
    };
    let value = match ClarityValue::consensus_deserialize(&mut Cursor::new(&value_bytes)) {
        Ok(value) => serialize_to_json(&value),
        Err(e) => {
            ctx.try_log(|logger| {
                slog::error!(logger, "unable to deserialize clarity value {:?}", e)
            });
            return json!(hex_value.to_string());
        }
    };
    value
}

pub fn serialize_to_json(value: &ClarityValue) -> serde_json::Value {
    match value {
        ClarityValue::Int(int) => json!(int),
        ClarityValue::UInt(int) => json!(int),
        ClarityValue::Bool(boolean) => json!(boolean),
        ClarityValue::Principal(principal_data) => json!(format!("{}", principal_data)),
        ClarityValue::Sequence(SequenceData::Buffer(vec_bytes)) => {
            json!(format!("0x{}", &vec_bytes))
        }
        ClarityValue::Sequence(SequenceData::String(CharType::ASCII(string))) => {
            json!(String::from_utf8(string.data.clone()).unwrap())
        }
        ClarityValue::Sequence(SequenceData::String(CharType::UTF8(string))) => {
            let mut result = String::new();
            for c in string.data.iter() {
                if c.len() > 1 {
                    result.push_str(&String::from_utf8(c.to_vec()).unwrap());
                } else {
                    result.push(c[0] as char)
                }
            }
            json!(result)
        }
        ClarityValue::Optional(opt_data) => match &opt_data.data {
            None => serde_json::Value::Null,
            Some(value) => serialize_to_json(&*value),
        },
        ClarityValue::Response(res_data) => {
            json!({
                "result": {
                    "success": res_data.committed,
                    "value": serialize_to_json(&*res_data.data),
                }
            })
        }
        ClarityValue::Tuple(data) => {
            let mut map = serde_json::Map::new();
            for (name, value) in data.data_map.iter() {
                map.insert(name.to_string(), serialize_to_json(value));
            }
            json!(map)
        }
        ClarityValue::Sequence(SequenceData::List(list_data)) => {
            let mut list = vec![];
            for value in list_data.data.iter() {
                list.push(serialize_to_json(value));
            }
            json!(list)
        }
        ClarityValue::CallableContract(callable) => {
            json!(format!("{}", callable.contract_identifier))
        }
    }
}

pub fn serialize_stacks_payload_to_json<'a>(
    trigger: StacksTriggerChainhook<'a>,
    _proofs: &HashMap<&'a TransactionIdentifier, String>,
    ctx: &Context,
) -> JsonValue {
    let decode_clarity_values = trigger.should_decode_clarity_value();
    let include_contract_abi = trigger.chainhook.include_contract_abi.unwrap_or(false);
    json!({
        "apply": trigger.apply.into_iter().map(|(transactions, block)| {
            serialize_stacks_block(block, transactions, decode_clarity_values, include_contract_abi, ctx)
        }).collect::<Vec<_>>(),
        "rollback": trigger.rollback.into_iter().map(|(transactions, block)| {
            serialize_stacks_block(block, transactions, decode_clarity_values, include_contract_abi, ctx)
        }).collect::<Vec<_>>(),
        "chainhook": {
            "uuid": trigger.chainhook.uuid,
            "predicate": trigger.chainhook.predicate,
            "is_streaming_blocks": trigger.chainhook.enabled
        }
    })
}

pub fn handle_stacks_hook_action<'a>(
    trigger: StacksTriggerChainhook<'a>,
    proofs: &HashMap<&'a TransactionIdentifier, String>,
    ctx: &Context,
) -> Result<StacksChainhookOccurrence, String> {
    match &trigger.chainhook.action {
        HookAction::HttpPost(http) => {
            let client = Client::builder()
                .build()
                .map_err(|e| format!("unable to build http client: {}", e.to_string()))?;
            let host = format!("{}", http.url);
            let method = Method::POST;
            let body = serde_json::to_vec(&serialize_stacks_payload_to_json(trigger, proofs, ctx))
                .map_err(|e| format!("unable to serialize payload {}", e.to_string()))?;
            Ok(StacksChainhookOccurrence::Http(
                client
                    .request(method, &host)
                    .header("Content-Type", "application/json")
                    .header("Authorization", http.authorization_header.clone())
                    .body(body),
            ))
        }
        HookAction::FileAppend(disk) => {
            let bytes = serde_json::to_vec(&serialize_stacks_payload_to_json(trigger, proofs, ctx))
                .map_err(|e| format!("unable to serialize payload {}", e.to_string()))?;
            Ok(StacksChainhookOccurrence::File(
                disk.path.to_string(),
                bytes,
            ))
        }
        HookAction::Noop => Ok(StacksChainhookOccurrence::Data(
            StacksChainhookOccurrencePayload {
                apply: trigger
                    .apply
                    .into_iter()
                    .map(|(transactions, block)| {
                        let transactions = transactions
                            .into_iter()
                            .map(|t| t.clone())
                            .collect::<Vec<_>>();
                        StacksApplyTransactionPayload {
                            block_identifier: block.get_identifier().clone(),
                            transactions,
                        }
                    })
                    .collect::<Vec<_>>(),
                rollback: trigger
                    .rollback
                    .into_iter()
                    .map(|(transactions, block)| {
                        let transactions = transactions
                            .into_iter()
                            .map(|t| t.clone())
                            .collect::<Vec<_>>();
                        StacksRollbackTransactionPayload {
                            block_identifier: block.get_identifier().clone(),
                            transactions,
                        }
                    })
                    .collect::<Vec<_>>(),
                chainhook: StacksChainhookPayload {
                    uuid: trigger.chainhook.uuid.clone(),
                },
            },
        )),
    }
}
