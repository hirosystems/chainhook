use crate::utils::{AbstractStacksBlock, Context};

use super::types::{BlockIdentifierIndexRule, ChainhookInstance, ExactMatchingRule, HookAction};
use chainhook_types::{
    BlockIdentifier, StacksChainEvent, StacksNetwork, StacksTransactionData,
    StacksTransactionEvent, StacksTransactionEventPayload, StacksTransactionKind,
    TransactionIdentifier,
};
use hiro_system_kit::slog;
use regex::Regex;
use reqwest::{Client, Method};
use schemars::JsonSchema;
use serde_json::Value as JsonValue;
use stacks_codec::clarity::codec::StacksMessageCodec;
use stacks_codec::clarity::vm::types::{CharType, SequenceData, Value as ClarityValue};
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use reqwest::RequestBuilder;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct StacksChainhookSpecification {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_after_occurrence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_all_events: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_clarity_values: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_contract_abi: Option<bool>,
    #[serde(rename = "if_this")]
    pub predicate: StacksPredicate,
    #[serde(rename = "then_that")]
    pub action: HookAction,
}

/// Maps some [StacksChainhookSpecification] to a corresponding [StacksNetwork]. This allows maintaining one
/// serialized predicate file for a given predicate on each network.
///
/// ### Examples
/// Given some file `predicate.json`:
/// ```json
/// {
///   "uuid": "my-id",
///   "name": "My Predicate",
///   "chain": "stacks",
///   "version": 1,
///   "networks": {
///     "devnet": {
///       // ...
///     },
///     "testnet": {
///       // ...
///     },
///     "mainnet": {
///       // ...
///     }
///   }
/// }
/// ```
/// You can deserialize the file to this type and create a [StacksChainhookInstance] for the desired network:
/// ```
/// use chainhook_sdk::chainhook::stacks::StacksChainhookSpecificationNetworkMap;
/// use chainhook_sdk::chainhook::stacks::StacksChainhookInstance;
/// use chainhook_types::StacksNetwork;
///
/// fn get_predicate(network: &StacksNetwork) -> Result<StacksChainhookInstance, String> {
///     let json_predicate =
///         std::fs::read_to_string("./predicate.json").expect("Unable to read file");
///     let hook_map: StacksChainhookSpecificationNetworkMap =
///         serde_json::from_str(&json_predicate).expect("Unable to parse Chainhook map");
///     hook_map.into_specification_for_network(network)
/// }
///
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct StacksChainhookSpecificationNetworkMap {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_uuid: Option<String>,
    pub name: String,
    pub version: u32,
    pub networks: BTreeMap<StacksNetwork, StacksChainhookSpecification>,
}

impl StacksChainhookSpecificationNetworkMap {
    pub fn into_specification_from_network(
        mut self,
        network: &StacksNetwork,
    ) -> Result<StacksChainhookInstance, String> {
        let spec = self
            .networks
            .remove(network)
            .ok_or("Network unknown".to_string())?;
        Ok(StacksChainhookInstance {
            uuid: self.uuid,
            owner_uuid: self.owner_uuid,
            name: self.name,
            network: network.clone(),
            version: self.version,
            start_block: spec.start_block,
            end_block: spec.end_block,
            blocks: spec.blocks,
            capture_all_events: spec.capture_all_events,
            decode_clarity_values: spec.decode_clarity_values,
            expire_after_occurrence: spec.expire_after_occurrence,
            include_contract_abi: spec.include_contract_abi,
            predicate: spec.predicate,
            action: spec.action,
            enabled: false,
            expired_at: None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StacksChainhookInstance {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_uuid: Option<String>,
    pub name: String,
    pub network: StacksNetwork,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_after_occurrence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_all_events: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_clarity_values: Option<bool>,
    pub include_contract_abi: Option<bool>,
    #[serde(rename = "predicate")]
    pub predicate: StacksPredicate,
    pub action: HookAction,
    pub enabled: bool,
    pub expired_at: Option<u64>,
}

impl StacksChainhookInstance {
    pub fn key(&self) -> String {
        ChainhookInstance::stacks_key(&self.uuid)
    }

    pub fn is_predicate_targeting_block_header(&self) -> bool {
        match &self.predicate {
            StacksPredicate::BlockHeight(_)
            // | &StacksPredicate::BitcoinBlockHeight(_)
            => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "scope")]
pub enum StacksPredicate {
    BlockHeight(BlockIdentifierIndexRule),
    ContractDeployment(StacksContractDeploymentPredicate),
    ContractCall(StacksContractCallBasedPredicate),
    PrintEvent(StacksPrintEventBasedPredicate),
    FtEvent(StacksFtEventBasedPredicate),
    NftEvent(StacksNftEventBasedPredicate),
    StxEvent(StacksStxEventBasedPredicate),
    Txid(ExactMatchingRule),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StacksContractCallBasedPredicate {
    pub contract_identifier: String,
    pub method: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
// #[serde(tag = "type", content = "rule")]
pub enum StacksContractDeploymentPredicate {
    Deployer(String),
    ImplementTrait(StacksTrait),
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StacksTrait {
    Sip09,
    Sip10,
    #[serde(rename = "*")]
    Any,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[serde(untagged)]
pub enum StacksPrintEventBasedPredicate {
    Contains {
        contract_identifier: String,
        contains: String,
    },
    MatchesRegex {
        contract_identifier: String,
        #[serde(rename = "matches_regex")]
        regex: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StacksFtEventBasedPredicate {
    pub asset_identifier: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StacksNftEventBasedPredicate {
    pub asset_identifier: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StacksStxEventBasedPredicate {
    pub actions: Vec<String>,
}

#[derive(Clone)]
pub struct StacksTriggerChainhook<'a> {
    pub chainhook: &'a StacksChainhookInstance,
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

impl StacksChainhookOccurrencePayload {
    pub fn from_trigger<'a>(
        trigger: StacksTriggerChainhook<'a>,
    ) -> StacksChainhookOccurrencePayload {
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
        }
    }
}
pub enum StacksChainhookOccurrence {
    Http(RequestBuilder, StacksChainhookOccurrencePayload),
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
    active_chainhooks: Vec<&'a StacksChainhookInstance>,
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
    chainhook: &'a StacksChainhookInstance,
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
    chainhook: &'a StacksChainhookInstance,
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
    chainhook: &'a StacksChainhookInstance,
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
                match (
                    &event.event_payload,
                    expecting_mint,
                    expecting_transfer,
                    expecting_burn,
                ) {
                    (StacksTransactionEventPayload::FTMintEvent(ft_event), true, _, _) => {
                        if ft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEventPayload::FTTransferEvent(ft_event), _, true, _) => {
                        if ft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEventPayload::FTBurnEvent(ft_event), _, _, true) => {
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
                match (
                    &event.event_payload,
                    expecting_mint,
                    expecting_transfer,
                    expecting_burn,
                ) {
                    (StacksTransactionEventPayload::NFTMintEvent(nft_event), true, _, _) => {
                        if nft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEventPayload::NFTTransferEvent(nft_event), _, true, _) => {
                        if nft_event
                            .asset_class_identifier
                            .eq(&expected_event.asset_identifier)
                        {
                            return true;
                        }
                    }
                    (StacksTransactionEventPayload::NFTBurnEvent(nft_event), _, _, true) => {
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
                    &event.event_payload,
                    expecting_mint,
                    expecting_transfer,
                    expecting_lock,
                    expecting_burn,
                ) {
                    (StacksTransactionEventPayload::STXMintEvent(_), true, _, _, _) => return true,
                    (StacksTransactionEventPayload::STXTransferEvent(_), _, true, _, _) => {
                        return true
                    }
                    (StacksTransactionEventPayload::STXLockEvent(_), _, _, true, _) => return true,
                    (StacksTransactionEventPayload::STXBurnEvent(_), _, _, _, true) => return true,
                    _ => continue,
                }
            }
            false
        }
        StacksPredicate::PrintEvent(expected_event) => {
            for event in transaction.metadata.receipt.events.iter() {
                match &event.event_payload {
                    StacksTransactionEventPayload::SmartContractEvent(actual) => {
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
    match &event.event_payload {
        StacksTransactionEventPayload::STXTransferEvent(payload) => {
            json!({
                "type": "STXTransferEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::STXMintEvent(payload) => {
            json!({
                "type": "STXMintEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::STXLockEvent(payload) => {
            json!({
                "type": "STXLockEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::STXBurnEvent(payload) => {
            json!({
                "type": "STXBurnEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::NFTTransferEvent(payload) => {
            json!({
                "type": "NFTTransferEvent",
                "data": {
                    "asset_class_identifier": payload.asset_class_identifier,
                    "asset_identifier": serialized_decoded_clarity_value(&payload.hex_asset_identifier, ctx),
                    "sender": payload.sender,
                    "recipient": payload.recipient,
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::NFTMintEvent(payload) => {
            json!({
                "type": "NFTMintEvent",
                "data": {
                    "asset_class_identifier": payload.asset_class_identifier,
                    "asset_identifier": serialized_decoded_clarity_value(&payload.hex_asset_identifier, ctx),
                    "recipient": payload.recipient,
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::NFTBurnEvent(payload) => {
            json!({
                "type": "NFTBurnEvent",
                "data": {
                    "asset_class_identifier": payload.asset_class_identifier,
                    "asset_identifier": serialized_decoded_clarity_value(&payload.hex_asset_identifier, ctx),
                    "sender": payload.sender,
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::FTTransferEvent(payload) => {
            json!({
                "type": "FTTransferEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::FTMintEvent(payload) => {
            json!({
                "type": "FTMintEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::FTBurnEvent(payload) => {
            json!({
                "type": "FTBurnEvent",
                "data": payload,
                "position": event.position
            })
        }
        StacksTransactionEventPayload::DataVarSetEvent(payload) => {
            json!({
                "type": "DataVarSetEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "var": payload.var,
                    "new_value": serialized_decoded_clarity_value(&payload.hex_new_value, ctx),
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::DataMapInsertEvent(payload) => {
            json!({
                "type": "DataMapInsertEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "map": payload.map,
                    "inserted_key": serialized_decoded_clarity_value(&payload.hex_inserted_key, ctx),
                    "inserted_value": serialized_decoded_clarity_value(&payload.hex_inserted_value, ctx),
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::DataMapUpdateEvent(payload) => {
            json!({
                "type": "DataMapUpdateEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "map": payload.map,
                    "key": serialized_decoded_clarity_value(&payload.hex_key, ctx),
                    "new_value": serialized_decoded_clarity_value(&payload.hex_new_value, ctx),
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::DataMapDeleteEvent(payload) => {
            json!({
                "type": "DataMapDeleteEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "map": payload.map,
                    "deleted_key": serialized_decoded_clarity_value(&payload.hex_deleted_key, ctx),
                },
                "position": event.position
            })
        }
        StacksTransactionEventPayload::SmartContractEvent(payload) => {
            json!({
                "type": "SmartContractEvent",
                "data": {
                    "contract_identifier": payload.contract_identifier,
                    "topic": payload.topic,
                    "value": serialized_decoded_clarity_value(&payload.hex_value, ctx),
                },
                "position": event.position
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
            let body = serde_json::to_vec(&serialize_stacks_payload_to_json(
                trigger.clone(),
                proofs,
                ctx,
            ))
            .map_err(|e| format!("unable to serialize payload {}", e.to_string()))?;
            Ok(StacksChainhookOccurrence::Http(
                client
                    .request(method, &host)
                    .header("Content-Type", "application/json")
                    .header("Authorization", http.authorization_header.clone())
                    .body(body),
                StacksChainhookOccurrencePayload::from_trigger(trigger),
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
            StacksChainhookOccurrencePayload::from_trigger(trigger),
        )),
    }
}
