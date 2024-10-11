use crate::StacksTransactionData;

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
    pub block: NakamotoBlockData,
    pub burn_height: u64,
    pub reward_cycle: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockAcceptedResponse {
    pub block_hash: String,
    pub sig: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum BlockValidationFailedCode {
    BadBlockHash = 0,
    BadTransaction = 1,
    InvalidBlock = 2,
    ChainstateError = 3,
    UnknownParent = 4,
    NonCanonicalTenure = 5,
    NoSuchTenure = 6,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
pub enum BlockResponseData {
    Accepted(BlockAcceptedResponse),
    Rejected(BlockRejectedResponse),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockPushedData {
    pub block: NakamotoBlockData,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum StacksSignerMessage {
    BlockProposal(BlockProposalData),
    BlockResponse(BlockResponseData),
    BlockPushed(BlockPushedData),
    MockProposal,
    MockSignature,
    MockBlock,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct StacksStackerDbChunk {
    pub contract: String,
    pub message: StacksSignerMessage,
    pub raw_data: String,
    pub raw_sig: String,
    pub slot_id: u64,
    pub slot_version: u64,
}
