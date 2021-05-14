#[derive(Clone, Debug)]
pub struct InitialContract {
    pub code: String,
    pub name: Option<String>,
    pub deployer: Option<String>,
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
    pub include_boot_contracts: Vec<String>,
    pub initial_links: Vec<InitialLink>,
    pub initial_contracts: Vec<InitialContract>,
    pub initial_accounts: Vec<Account>,
    pub initial_deployer: Option<Account>,
}
