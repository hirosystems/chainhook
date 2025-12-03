use chainhook_sdk::indexer::database::BlocksDatabaseAccess;
use chainhook_sdk::types::BlockIdentifier;
use chainhook_sdk::utils::Context;
use std::path::PathBuf;

use crate::storage::{get_stacks_block_at_block_height, open_readonly_stacks_db_conn_with_retry};

/// Implementation of DatabaseAccess trait for chainhook-cli
/// This provides database access to the SDK without creating circular dependencies
pub struct StacksDatabaseAccess {
    db_path: PathBuf,
}

impl StacksDatabaseAccess {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl BlocksDatabaseAccess for StacksDatabaseAccess {
    fn block_exists(
        &self,
        block_identifier: &BlockIdentifier,
        ctx: &Context,
    ) -> Result<bool, String> {
        let stacks_db = open_readonly_stacks_db_conn_with_retry(&self.db_path, 0, ctx)?;
        // We have to retrieve the full block to check and compare both its hash and index. There's a function called
        // `is_stacks_block_present` that sounds like could be used here but it only checks the index.
        if let Some(block) =
            get_stacks_block_at_block_height(block_identifier.index, true, 0, &stacks_db)?
        {
            if block.block_identifier.hash == block_identifier.hash {
                return Ok(true);
            }
        }
        return Ok(false);
    }
}
