use std::collections::BTreeMap;

use chainhook_types::StacksNetwork;
use schemars::JsonSchema;

use super::{BlockIdentifierIndexRule, ChainhookSpecification, ExactMatchingRule, HookAction};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct StacksChainhookFullSpecification {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_uuid: Option<String>,
    pub name: String,
    pub version: u32,
    pub networks: BTreeMap<StacksNetwork, StacksChainhookNetworkSpecification>,
}

impl StacksChainhookFullSpecification {
    pub fn into_selected_network_specification(
        mut self,
        network: &StacksNetwork,
        enabled: Option<bool>,
    ) -> Result<StacksChainhookSpecification, String> {
        let spec = self
            .networks
            .remove(network)
            .ok_or("Network unknown".to_string())?;
        Ok(StacksChainhookSpecification {
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
            enabled: enabled.unwrap_or(false),
            expired_at: None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct StacksChainhookNetworkSpecification {
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StacksChainhookSpecification {
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

impl StacksChainhookSpecification {
    pub fn key(&self) -> String {
        ChainhookSpecification::stacks_key(&self.uuid)
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
