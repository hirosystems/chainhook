use super::types::{
    BitcoinChainhookSpecification, BitcoinPredicateType, DescriptorMatchingRule, ExactMatchingRule,
    HookAction, InputPredicate, MatchingRule, OrdinalOperations, OutputPredicate, StacksOperations,
};
use crate::utils::Context;

use bitcoincore_rpc_json::bitcoin::{address::Payload, Address};
use chainhook_types::{
    BitcoinBlockData, BitcoinChainEvent, BitcoinTransactionData, BlockIdentifier, OrdinalOperation,
    StacksBaseChainOperation, TransactionIdentifier,
};

use hiro_system_kit::slog;

use miniscript::bitcoin::secp256k1::Secp256k1;
use miniscript::Descriptor;

use reqwest::{Client, Method};
use serde_json::Value as JsonValue;
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
};

use reqwest::RequestBuilder;

use hex::FromHex;

pub struct BitcoinTriggerChainhook<'a> {
    pub chainhook: &'a BitcoinChainhookSpecification,
    pub apply: Vec<(Vec<&'a BitcoinTransactionData>, &'a BitcoinBlockData)>,
    pub rollback: Vec<(Vec<&'a BitcoinTransactionData>, &'a BitcoinBlockData)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BitcoinTransactionPayload {
    pub block: BitcoinBlockData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BitcoinChainhookPayload {
    pub uuid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BitcoinChainhookOccurrencePayload {
    pub apply: Vec<BitcoinTransactionPayload>,
    pub rollback: Vec<BitcoinTransactionPayload>,
    pub chainhook: BitcoinChainhookPayload,
}

impl BitcoinChainhookOccurrencePayload {
    pub fn from_trigger<'a>(
        trigger: BitcoinTriggerChainhook<'a>,
    ) -> BitcoinChainhookOccurrencePayload {
        BitcoinChainhookOccurrencePayload {
            apply: trigger
                .apply
                .into_iter()
                .map(|(transactions, block)| {
                    let mut block = block.clone();
                    block.transactions = transactions
                        .into_iter()
                        .map(|t| t.clone())
                        .collect::<Vec<_>>();
                    BitcoinTransactionPayload { block }
                })
                .collect::<Vec<_>>(),
            rollback: trigger
                .rollback
                .into_iter()
                .map(|(transactions, block)| {
                    let mut block = block.clone();
                    block.transactions = transactions
                        .into_iter()
                        .map(|t| t.clone())
                        .collect::<Vec<_>>();
                    BitcoinTransactionPayload { block }
                })
                .collect::<Vec<_>>(),
            chainhook: BitcoinChainhookPayload {
                uuid: trigger.chainhook.uuid.clone(),
            },
        }
    }
}

pub enum BitcoinChainhookOccurrence {
    Http(RequestBuilder, BitcoinChainhookOccurrencePayload),
    File(String, Vec<u8>),
    Data(BitcoinChainhookOccurrencePayload),
}

pub fn evaluate_bitcoin_chainhooks_on_chain_event<'a>(
    chain_event: &'a BitcoinChainEvent,
    active_chainhooks: &Vec<&'a BitcoinChainhookSpecification>,
    ctx: &Context,
) -> (
    Vec<BitcoinTriggerChainhook<'a>>,
    BTreeMap<&'a str, &'a BlockIdentifier>,
    BTreeMap<&'a str, &'a BlockIdentifier>,
) {
    let mut evaluated_predicates = BTreeMap::new();
    let mut triggered_predicates = vec![];
    let mut expired_predicates = BTreeMap::new();

    match chain_event {
        BitcoinChainEvent::ChainUpdatedWithBlocks(event) => {
            for chainhook in active_chainhooks.iter() {
                let mut apply = vec![];
                let rollback = vec![];
                let end_block = chainhook.end_block.unwrap_or(u64::MAX);

                for block in event.new_blocks.iter() {
                    evaluated_predicates.insert(chainhook.uuid.as_str(), &block.block_identifier);
                    if end_block >= block.block_identifier.index {
                        let mut hits = vec![];
                        for tx in block.transactions.iter() {
                            if chainhook.predicate.evaluate_transaction_predicate(&tx, ctx) {
                                hits.push(tx);
                            }
                        }
                        if hits.len() > 0 {
                            apply.push((hits, block));
                        }
                    } else {
                        expired_predicates.insert(chainhook.uuid.as_str(), &block.block_identifier);
                    }
                }

                if !apply.is_empty() {
                    triggered_predicates.push(BitcoinTriggerChainhook {
                        chainhook,
                        apply,
                        rollback,
                    })
                }
            }
        }
        BitcoinChainEvent::ChainUpdatedWithReorg(event) => {
            for chainhook in active_chainhooks.iter() {
                let mut apply = vec![];
                let mut rollback = vec![];
                let end_block = chainhook.end_block.unwrap_or(u64::MAX);

                for block in event.blocks_to_rollback.iter() {
                    if end_block >= block.block_identifier.index {
                        let mut hits = vec![];
                        for tx in block.transactions.iter() {
                            if chainhook.predicate.evaluate_transaction_predicate(&tx, ctx) {
                                hits.push(tx);
                            }
                        }
                        if hits.len() > 0 {
                            rollback.push((hits, block));
                        }
                    } else {
                        expired_predicates.insert(chainhook.uuid.as_str(), &block.block_identifier);
                    }
                }
                for block in event.blocks_to_apply.iter() {
                    evaluated_predicates.insert(chainhook.uuid.as_str(), &block.block_identifier);
                    if end_block >= block.block_identifier.index {
                        let mut hits = vec![];
                        for tx in block.transactions.iter() {
                            if chainhook.predicate.evaluate_transaction_predicate(&tx, ctx) {
                                hits.push(tx);
                            }
                        }
                        if hits.len() > 0 {
                            apply.push((hits, block));
                        }
                    } else {
                        expired_predicates.insert(chainhook.uuid.as_str(), &block.block_identifier);
                    }
                }
                if !apply.is_empty() || !rollback.is_empty() {
                    triggered_predicates.push(BitcoinTriggerChainhook {
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

pub fn serialize_bitcoin_payload_to_json<'a>(
    trigger: &BitcoinTriggerChainhook<'a>,
    proofs: &HashMap<&'a TransactionIdentifier, String>,
) -> JsonValue {
    let predicate_spec = trigger.chainhook;
    json!({
        "apply": trigger.apply.iter().map(|(transactions, block)| {
            json!({
                "block_identifier": block.block_identifier,
                "parent_block_identifier": block.parent_block_identifier,
                "timestamp": block.timestamp,
                "transactions": serialize_bitcoin_transactions_to_json(&predicate_spec, &transactions, proofs),
                "metadata": block.metadata,
            })
        }).collect::<Vec<_>>(),
        "rollback": trigger.rollback.iter().map(|(transactions, block)| {
            json!({
                "block_identifier": block.block_identifier,
                "parent_block_identifier": block.parent_block_identifier,
                "timestamp": block.timestamp,
                "transactions": serialize_bitcoin_transactions_to_json(&predicate_spec, &transactions, proofs),
                "metadata": block.metadata,
            })
        }).collect::<Vec<_>>(),
        "chainhook": {
            "uuid": trigger.chainhook.uuid,
            "predicate": trigger.chainhook.predicate,
            "is_streaming_blocks": trigger.chainhook.enabled
        }
    })
}

pub fn serialize_bitcoin_transactions_to_json<'a>(
    predicate_spec: &BitcoinChainhookSpecification,
    transactions: &Vec<&BitcoinTransactionData>,
    proofs: &HashMap<&'a TransactionIdentifier, String>,
) -> Vec<JsonValue> {
    transactions
        .into_iter()
        .map(|transaction| {
            let mut metadata = serde_json::Map::new();
            if predicate_spec.include_inputs {
                metadata.insert(
                    "inputs".into(),
                    json!(transaction
                        .metadata
                        .inputs
                        .iter()
                        .map(|input| {
                            json!({
                                "txin": input.previous_output.txid.hash.to_string(),
                                "vout": input.previous_output.vout,
                                "sequence": input.sequence,
                            })
                        })
                        .collect::<Vec<_>>()),
                );
            }
            if predicate_spec.include_outputs {
                metadata.insert("outputs".into(), json!(transaction.metadata.outputs));
            }
            if !transaction.metadata.stacks_operations.is_empty() {
                metadata.insert(
                    "stacks_operations".into(),
                    json!(transaction.metadata.stacks_operations),
                );
            }
            if !transaction.metadata.ordinal_operations.is_empty() {
                metadata.insert(
                    "ordinal_operations".into(),
                    json!(transaction.metadata.ordinal_operations),
                );
            }
            metadata.insert(
                "proof".into(),
                json!(proofs.get(&transaction.transaction_identifier)),
            );
            json!({
                "transaction_identifier": transaction.transaction_identifier,
                "operations": transaction.operations,
                "metadata": metadata
            })
        })
        .collect::<Vec<_>>()
}

pub fn handle_bitcoin_hook_action<'a>(
    trigger: BitcoinTriggerChainhook<'a>,
    proofs: &HashMap<&'a TransactionIdentifier, String>,
) -> Result<BitcoinChainhookOccurrence, String> {
    match &trigger.chainhook.action {
        HookAction::HttpPost(http) => {
            let client = Client::builder()
                .build()
                .map_err(|e| format!("unable to build http client: {}", e.to_string()))?;
            let host = format!("{}", http.url);
            let method = Method::POST;
            let body = serde_json::to_vec(&serialize_bitcoin_payload_to_json(&trigger, proofs))
                .map_err(|e| format!("unable to serialize payload {}", e.to_string()))?;
            let request = client
                .request(method, &host)
                .header("Content-Type", "application/json")
                .header("Authorization", http.authorization_header.clone())
                .body(body);

            let data = BitcoinChainhookOccurrencePayload::from_trigger(trigger);
            Ok(BitcoinChainhookOccurrence::Http(request, data))
        }
        HookAction::FileAppend(disk) => {
            let bytes = serde_json::to_vec(&serialize_bitcoin_payload_to_json(&trigger, proofs))
                .map_err(|e| format!("unable to serialize payload {}", e.to_string()))?;
            Ok(BitcoinChainhookOccurrence::File(
                disk.path.to_string(),
                bytes,
            ))
        }
        HookAction::Noop => Ok(BitcoinChainhookOccurrence::Data(
            BitcoinChainhookOccurrencePayload::from_trigger(trigger),
        )),
    }
}

struct OpReturn(String);
impl OpReturn {
    fn from_string(hex: &String) -> Result<String, String> {
        // Remove the `0x` prefix if present so that we can call from_hex without errors.
        let hex = hex.strip_prefix("0x").unwrap_or(hex);

        // Parse the hex bytes.
        let bytes = Vec::<u8>::from_hex(hex).unwrap();
        match bytes.as_slice() {
            // An OpReturn is composed by:
            // - OP_RETURN 0x6a
            // - Data length <N> (ignored)
            // - The data
            [0x6a, _, rest @ ..] => Ok(hex::encode(rest)),
            _ => Err(String::from("not an OP_RETURN")),
        }
    }
}

impl BitcoinPredicateType {
    pub fn evaluate_transaction_predicate(
        &self,
        tx: &BitcoinTransactionData,
        ctx: &Context,
    ) -> bool {
        // TODO(lgalabru): follow-up on this implementation
        match &self {
            BitcoinPredicateType::Block => true,
            BitcoinPredicateType::Txid(ExactMatchingRule::Equals(txid)) => {
                tx.transaction_identifier.hash.eq(txid)
            }
            BitcoinPredicateType::Outputs(OutputPredicate::OpReturn(rule)) => {
                for output in tx.metadata.outputs.iter() {
                    // opret contains the op_return data section prefixed with `0x`.
                    let opret = match OpReturn::from_string(&output.script_pubkey) {
                        Ok(op) => op,
                        Err(_) => continue,
                    };

                    // encoded_pattern takes a predicate pattern and return its lowercase hex
                    // representation.
                    fn encoded_pattern(pattern: &str) -> String {
                        // If the pattern starts with 0x, return it in lowercase and without the 0x
                        // prefix.
                        if pattern.starts_with("0x") {
                            return pattern
                                .strip_prefix("0x")
                                .unwrap()
                                .to_lowercase()
                                .to_string();
                        }

                        // In this case it should be trated as ASCII so let's return its hex
                        // representation.
                        hex::encode(pattern)
                    }

                    match rule {
                        MatchingRule::StartsWith(pattern) => {
                            if opret.starts_with(&encoded_pattern(pattern)) {
                                return true;
                            }
                        }
                        MatchingRule::EndsWith(pattern) => {
                            if opret.ends_with(&encoded_pattern(pattern)) {
                                return true;
                            }
                        }
                        MatchingRule::Equals(pattern) => {
                            if opret.eq(&encoded_pattern(pattern)) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            BitcoinPredicateType::Outputs(OutputPredicate::P2pkh(ExactMatchingRule::Equals(
                encoded_address,
            )))
            | BitcoinPredicateType::Outputs(OutputPredicate::P2sh(ExactMatchingRule::Equals(
                encoded_address,
            ))) => {
                let address = match Address::from_str(encoded_address) {
                    Ok(address) => address.assume_checked(),
                    Err(_) => return false,
                };
                let address_bytes = hex::encode(address.script_pubkey().as_bytes());
                for output in tx.metadata.outputs.iter() {
                    if output.script_pubkey[2..] == address_bytes {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::Outputs(OutputPredicate::P2wpkh(ExactMatchingRule::Equals(
                encoded_address,
            )))
            | BitcoinPredicateType::Outputs(OutputPredicate::P2wsh(ExactMatchingRule::Equals(
                encoded_address,
            ))) => {
                let address = match Address::from_str(encoded_address) {
                    Ok(address) => {
                        let checked_address = address.assume_checked();
                        match checked_address.payload() {
                            Payload::WitnessProgram(_) => checked_address,
                            _ => return false,
                        }
                    }
                    Err(_) => return false,
                };
                let address_bytes = hex::encode(address.script_pubkey().as_bytes());
                for output in tx.metadata.outputs.iter() {
                    if output.script_pubkey[2..] == address_bytes {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::Outputs(OutputPredicate::Descriptor(
                DescriptorMatchingRule { expression, range },
            )) => {
                // To derive from descriptors, we need to provide a secp context.
                let (sig, ver) = (&Secp256k1::signing_only(), &Secp256k1::verification_only());
                let (desc, _) = Descriptor::parse_descriptor(&sig, expression).unwrap();

                // If the descriptor is derivable (`has_wildcard()`), we rely on the `range` field
                // defined by the predicate OR fallback to a default range of [0,5] when not set.
                // When the descriptor is not derivable we force to create a unique iteration by
                // ranging over [0,1].
                let range = if desc.has_wildcard() {
                    range.unwrap_or([0, 5])
                } else {
                    [0, 1]
                };

                // Derive the addresses and try to match them against the outputs.
                for i in range[0]..range[1] {
                    let derived = desc.derived_descriptor(&ver, i).unwrap();

                    // Extract and encode the derived pubkey.
                    let script_pubkey = hex::encode(derived.script_pubkey().as_bytes());

                    // Match that script against the tx outputs.
                    for (index, output) in tx.metadata.outputs.iter().enumerate() {
                        if output.script_pubkey[2..] == script_pubkey {
                            ctx.try_log(|logger| {
                                slog::debug!(
                                    logger,
                                    "Descriptor: Matched pubkey {:?} on tx {:?} output {}",
                                    script_pubkey,
                                    tx.transaction_identifier.get_hash_bytes_str(),
                                    index,
                                )
                            });

                            return true;
                        }
                    }
                }

                false
            }
            BitcoinPredicateType::Inputs(InputPredicate::Txid(predicate)) => {
                // TODO(lgalabru): add support for transaction chainhing, if enabled
                for input in tx.metadata.inputs.iter() {
                    if input.previous_output.txid.hash.eq(&predicate.txid)
                        && input.previous_output.vout.eq(&predicate.vout)
                    {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::Inputs(InputPredicate::WitnessScript(_)) => {
                // TODO(lgalabru)
                unimplemented!()
            }
            BitcoinPredicateType::StacksProtocol(StacksOperations::StackerRewarded) => {
                for op in tx.metadata.stacks_operations.iter() {
                    if let StacksBaseChainOperation::BlockCommitted(_) = op {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::StacksProtocol(StacksOperations::BlockCommitted) => {
                for op in tx.metadata.stacks_operations.iter() {
                    if let StacksBaseChainOperation::BlockCommitted(_) = op {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::StacksProtocol(StacksOperations::LeaderRegistered) => {
                for op in tx.metadata.stacks_operations.iter() {
                    if let StacksBaseChainOperation::LeaderRegistered(_) = op {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::StacksProtocol(StacksOperations::StxTransferred) => {
                for op in tx.metadata.stacks_operations.iter() {
                    if let StacksBaseChainOperation::StxTransferred(_) = op {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::StacksProtocol(StacksOperations::StxLocked) => {
                for op in tx.metadata.stacks_operations.iter() {
                    if let StacksBaseChainOperation::StxLocked(_) = op {
                        return true;
                    }
                }
                false
            }
            BitcoinPredicateType::OrdinalsProtocol(OrdinalOperations::InscriptionFeed) => {
                for op in tx.metadata.ordinal_operations.iter() {
                    match op {
                        OrdinalOperation::InscriptionRevealed(_)
                        | OrdinalOperation::InscriptionTransferred(_) => return true,
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
pub mod tests;
