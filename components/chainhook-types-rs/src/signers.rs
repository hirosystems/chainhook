use crate::{BlockIdentifier, StacksTransactionData};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct NakamotoBlockHeaderData {
    pub version: u8,
    pub chain_length: u64,
    pub burn_spent: u64,
    pub consensus_hash: String,
    pub parent_block_id: String,
    pub tx_merkle_root: String,
    pub state_index_root: String,
    pub timestamp: u64,
    pub miner_signature: String,
    pub signer_signature: Vec<String>,
    pub pox_treatment: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct NakamotoBlockData {
    pub header: NakamotoBlockHeaderData,
    pub transactions: Vec<StacksTransactionData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockProposalData {
    // TODO(rafaelcr): Include `block_hash` and `index_block_hash`.
    pub block: NakamotoBlockData,
    pub burn_height: u64,
    pub reward_cycle: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockAcceptedResponse {
    pub signer_signature_hash: String,
    pub sig: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum BlockValidationFailedCode {
    BadBlockHash,
    BadTransaction,
    InvalidBlock,
    ChainstateError,
    UnknownParent,
    NonCanonicalTenure,
    NoSuchTenure,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockRejectReasonCode {
    ValidationFailed(BlockValidationFailedCode),
    ConnectivityIssues,
    RejectedInPriorRound,
    NoSortitionView,
    SortitionViewMismatch,
    TestingDirective,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockRejectedResponse {
    pub reason: String,
    pub reason_code: BlockRejectReasonCode,
    pub signer_signature_hash: String,
    pub chain_id: u32,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum BlockResponseData {
    Accepted(BlockAcceptedResponse),
    Rejected(BlockRejectedResponse),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockPushedData {
    pub block: NakamotoBlockData,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum StacksSignerMessage {
    BlockProposal(BlockProposalData),
    BlockResponse(BlockResponseData),
    BlockPushed(BlockPushedData),
    // TODO(rafaelcr): Add mock messages
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct StacksStackerDbChunk {
    pub contract: String,
    pub sig: String,
    pub pubkey: String,
    pub message: StacksSignerMessage,
    pub received_at: u64,
    pub received_at_block: BlockIdentifier,
}
