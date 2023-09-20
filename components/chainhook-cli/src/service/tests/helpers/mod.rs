use std::net::TcpListener;

pub mod build_predicates;
pub mod mock_bitcoin_rpc;
pub mod mock_service;
pub mod mock_stacks_node;

pub fn height_to_prefixed_hash(height: u64) -> String {
    format!("0x{}", height_to_hash_str(height))
}
fn height_to_hash_str(height: u64) -> String {
    format!("{:0>64}", height.to_string())
}

pub fn branch_and_height_to_prefixed_hash(branch: Option<char>, height: u64) -> String {
    format!("0x{}", branch_and_height_to_hash_str(branch, height))
}
fn branch_and_height_to_hash_str(branch: Option<char>, height: u64) -> String {
    let branch = branch.unwrap_or('0');
    format!("{branch}{:0>63}", height.to_string())
}

pub fn get_free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Failed to bind to port 0: {}", e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to parse address: {}", e.to_string()))?
        .port();
    drop(listener);
    Ok(port)
}
