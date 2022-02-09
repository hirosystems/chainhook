use std::convert::TryInto;

use crate::clarity::{
    coverage::CoverageReporter,
    types::{PrincipalData, QualifiedContractIdentifier, StandardPrincipalData},
    util::StacksAddress,
};

#[derive(Clone, Debug)]
pub struct InitialContract {
    pub code: String,
    pub name: Option<String>,
    pub path: String,
    pub deployer: Option<String>,
}

impl InitialContract {
    pub fn get_contract_identifier(&self, is_mainnet: bool) -> Option<QualifiedContractIdentifier> {
        match self.name {
            Some(ref name) => Some(QualifiedContractIdentifier {
                issuer: self.get_deployer_principal(is_mainnet).into(),
                name: name.to_string().try_into().unwrap(),
            }),
            _ => None,
        }
    }

    pub fn get_deployer_principal(&self, is_mainnet: bool) -> StandardPrincipalData {
        let address = match self.deployer {
            Some(ref entry) => entry.clone(),
            None => format!("{}", StacksAddress::burn_address(is_mainnet)),
        };
        PrincipalData::parse_standard_principal(&address)
            .expect("Unable to parse deployer's address")
    }
}

#[derive(Clone, Debug)]
pub struct InitialLink {
    pub contract_id: String,
    pub stacks_node_addr: Option<String>,
    pub cache: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub address: String,
    pub balance: u64,
    pub name: String,
    pub mnemonic: String,
    pub derivation: String,
}

#[derive(Clone, Debug, Default)]
pub struct SessionSettings {
    pub node: String,
    pub include_boot_contracts: Vec<String>,
    pub include_costs: bool,
    pub costs_version: u32,
    pub initial_links: Vec<InitialLink>,
    pub initial_contracts: Vec<InitialContract>,
    pub initial_accounts: Vec<Account>,
    pub initial_deployer: Option<Account>,
    pub scoping_contract: Option<String>,
    pub analysis: Vec<String>,
    pub lazy_initial_contracts_interpretation: bool,
    pub parser_version: u32,
    pub disk_cache_enabled: bool,
}
