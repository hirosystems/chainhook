use std::collections::{BTreeMap, HashSet};

use chainhook_types::BitcoinNetwork;
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer};

use super::{opcode_to_hex, ChainhookSpecification, ExactMatchingRule, HookAction, MatchingRule};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BitcoinChainhookSpecification {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_uuid: Option<String>,
    pub name: String,
    pub network: BitcoinNetwork,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_after_occurrence: Option<u64>,
    pub predicate: BitcoinPredicateType,
    pub action: HookAction,
    pub include_proof: bool,
    pub include_inputs: bool,
    pub include_outputs: bool,
    pub include_witness: bool,
    pub enabled: bool,
    pub expired_at: Option<u64>,
}

impl BitcoinChainhookSpecification {
    pub fn key(&self) -> String {
        ChainhookSpecification::bitcoin_key(&self.uuid)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct BitcoinChainhookFullSpecification {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_uuid: Option<String>,
    pub name: String,
    pub version: u32,
    pub networks: BTreeMap<BitcoinNetwork, BitcoinChainhookNetworkSpecification>,
}

impl BitcoinChainhookFullSpecification {
    pub fn into_selected_network_specification(
        mut self,
        network: &BitcoinNetwork,
        enabled: Option<bool>,
    ) -> Result<BitcoinChainhookSpecification, String> {
        let spec = self
            .networks
            .remove(network)
            .ok_or("Network unknown".to_string())?;
        Ok(BitcoinChainhookSpecification {
            uuid: self.uuid,
            owner_uuid: self.owner_uuid,
            name: self.name,
            network: network.clone(),
            version: self.version,
            start_block: spec.start_block,
            end_block: spec.end_block,
            blocks: spec.blocks,
            expire_after_occurrence: spec.expire_after_occurrence,
            predicate: spec.predicate,
            action: spec.action,
            include_proof: spec.include_proof.unwrap_or(false),
            include_inputs: spec.include_inputs.unwrap_or(false),
            include_outputs: spec.include_outputs.unwrap_or(false),
            include_witness: spec.include_witness.unwrap_or(false),
            enabled: enabled.unwrap_or(false),
            expired_at: None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct BitcoinChainhookNetworkSpecification {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_after_occurrence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_proof: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_inputs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_outputs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_witness: Option<bool>,
    #[serde(rename = "if_this")]
    pub predicate: BitcoinPredicateType,
    #[serde(rename = "then_that")]
    pub action: HookAction,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ScriptTemplate {
    pub instructions: Vec<ScriptInstruction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScriptInstruction {
    Opcode(u8),
    RawBytes(Vec<u8>),
    Placeholder(String, u8),
}

impl ScriptTemplate {
    pub fn parse(template: &str) -> Result<ScriptTemplate, String> {
        let raw_instructions = template
            .split_ascii_whitespace()
            .map(|c| c.to_string())
            .collect::<Vec<_>>();
        let mut instructions = vec![];
        for raw_instruction in raw_instructions.into_iter() {
            if raw_instruction.starts_with("{") {
                let placeholder = &raw_instruction[1..raw_instruction.len() - 1];
                let (name, size) = match placeholder.split_once(":") {
                    Some(res) => res,
                    None => return Err(format!("malformed placeholder {}: should be {{placeholder-name:number-of-bytes}} (ex: {{id:4}}", raw_instruction))
                };
                let size = match size.parse::<u8>() {
                    Ok(res) => res,
                    Err(_) => return Err(format!("malformed placeholder {}: should be {{placeholder-name:number-of-bytes}} (ex: {{id:4}}", raw_instruction))
                };
                instructions.push(ScriptInstruction::Placeholder(name.to_string(), size));
            } else if let Some(opcode) = opcode_to_hex(&raw_instruction) {
                instructions.push(ScriptInstruction::Opcode(opcode));
            } else if let Ok(bytes) = hex::decode(&raw_instruction) {
                instructions.push(ScriptInstruction::RawBytes(bytes));
            } else {
                return Err(format!("unable to handle instruction {}", raw_instruction));
            }
        }
        Ok(ScriptTemplate { instructions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct BitcoinTransactionFilterPredicate {
    pub predicate: BitcoinPredicateType,
}

impl BitcoinTransactionFilterPredicate {
    pub fn new(predicate: BitcoinPredicateType) -> BitcoinTransactionFilterPredicate {
        BitcoinTransactionFilterPredicate { predicate }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "scope")]
pub enum BitcoinPredicateType {
    Block,
    Txid(ExactMatchingRule),
    Inputs(InputPredicate),
    Outputs(OutputPredicate),
    StacksProtocol(StacksOperations),
    OrdinalsProtocol(OrdinalOperations),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputPredicate {
    Txid(TxinPredicate),
    WitnessScript(MatchingRule),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputPredicate {
    OpReturn(MatchingRule),
    P2pkh(ExactMatchingRule),
    P2sh(ExactMatchingRule),
    P2wpkh(ExactMatchingRule),
    P2wsh(ExactMatchingRule),
    Descriptor(DescriptorMatchingRule),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "operation")]
pub enum StacksOperations {
    StackerRewarded,
    BlockCommitted,
    LeaderRegistered,
    StxTransferred,
    StxLocked,
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OrdinalsMetaProtocol {
    All,
    #[serde(rename = "brc-20")]
    Brc20,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct InscriptionFeedData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_protocols: Option<HashSet<OrdinalsMetaProtocol>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "operation")]
pub enum OrdinalOperations {
    InscriptionFeed(InscriptionFeedData),
}

pub fn get_stacks_canonical_magic_bytes(network: &BitcoinNetwork) -> [u8; 2] {
    match network {
        BitcoinNetwork::Mainnet => *b"X2",
        BitcoinNetwork::Testnet => *b"T2",
        BitcoinNetwork::Regtest => *b"id",
        BitcoinNetwork::Signet => unreachable!(),
    }
}
#[derive(Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum StacksOpcodes {
    BlockCommit = '[' as u8,
    KeyRegister = '^' as u8,
    StackStx = 'x' as u8,
    PreStx = 'p' as u8,
    TransferStx = '$' as u8,
}

impl TryFrom<u8> for StacksOpcodes {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == StacksOpcodes::BlockCommit as u8 => Ok(StacksOpcodes::BlockCommit),
            x if x == StacksOpcodes::KeyRegister as u8 => Ok(StacksOpcodes::KeyRegister),
            x if x == StacksOpcodes::StackStx as u8 => Ok(StacksOpcodes::StackStx),
            x if x == StacksOpcodes::PreStx as u8 => Ok(StacksOpcodes::PreStx),
            x if x == StacksOpcodes::TransferStx as u8 => Ok(StacksOpcodes::TransferStx),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TxinPredicate {
    pub txid: String,
    pub vout: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Inputs,
    Outputs,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DescriptorMatchingRule {
    // expression defines the bitcoin descriptor.
    pub expression: String,
    #[serde(default, deserialize_with = "deserialize_descriptor_range")]
    pub range: Option<[u32; 2]>,
}

// deserialize_descriptor_range makes sure that the range value is valid.
fn deserialize_descriptor_range<'de, D>(deserializer: D) -> Result<Option<[u32; 2]>, D::Error>
where
    D: Deserializer<'de>,
{
    let range: [u32; 2] = Deserialize::deserialize(deserializer)?;
    if !(range[0] < range[1]) {
        Err(de::Error::custom(
            "First element of 'range' must be lower than the second element",
        ))
    } else {
        Ok(Some(range))
    }
}
