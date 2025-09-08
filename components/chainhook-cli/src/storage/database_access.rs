use chainhook_sdk::indexer::database::BlocksDatabaseAccess;
use chainhook_sdk::types::BlockIdentifier;
use chainhook_sdk::utils::Context;
use std::path::PathBuf;

use crate::storage::{is_stacks_block_present, open_readonly_stacks_db_conn_with_retry};

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
        Ok(is_stacks_block_present(block_identifier, 0, &stacks_db))
    }
}
