use chainhook_types::StacksBlockData;
use std::collections::HashMap;

lazy_static! {
    pub static ref TESTNET_STACKS_BLOCK_FIXTURES: HashMap<u64, StacksBlockData> = {
        let mut fixtures: HashMap<u64, StacksBlockData> = HashMap::new();
        fixtures.insert(
            107605,
            load_stacks_block_fixture(std::include_str!("stacks/testnet/107605.json")),
        );
        fixtures.insert(
            20835,
            load_stacks_block_fixture(std::include_str!("stacks/testnet/20835.json")),
        );
        fixtures.insert(
            20898,
            load_stacks_block_fixture(std::include_str!("stacks/testnet/20898.json")),
        );
        fixtures
    };
}

pub fn load_stacks_block_fixture(json_str: &str) -> StacksBlockData {
    serde_json::from_str(json_str).unwrap()
}

pub fn get_stacks_testnet_block(block_height: u64) -> &'static StacksBlockData {
    TESTNET_STACKS_BLOCK_FIXTURES.get(&block_height).unwrap()
}
