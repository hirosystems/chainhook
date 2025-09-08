use crate::utils::Context;
use chainhook_types::BlockIdentifier;

/// Trait for database operations needed by the indexer
/// This allows the SDK to work with different database implementations
/// without creating circular dependencies
pub trait BlocksDatabaseAccess {
    /// Check if a block exists in the database
    fn block_exists(&self, block_identifier: &BlockIdentifier, ctx: &Context) -> Result<bool, String>;
}

impl BlocksDatabaseAccess for () {
    fn block_exists(&self, _block_identifier: &BlockIdentifier, _ctx: &Context) -> Result<bool, String> {
        Ok(false)
    }
}
