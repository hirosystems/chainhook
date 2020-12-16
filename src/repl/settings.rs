#[derive(Clone, Debug)]
pub struct InitialContract {
    pub code: String,
    pub name: Option<String>,
    pub deployer: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InitialBalance {
    pub address: String,
    pub amount: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SessionSettings {
    pub initial_contracts: Vec<InitialContract>,
    pub initial_balances: Vec<InitialBalance>,
}
