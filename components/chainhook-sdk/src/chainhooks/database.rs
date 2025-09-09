use crate::chainhooks::bitcoin::BitcoinChainhookInstance;
use crate::chainhooks::stacks::StacksChainhookInstance;
use crate::chainhooks::types::PredicateStatus;
use crate::utils::Context;

/// Trait for predicate storage operations needed by the SDK
/// This allows the SDK to work with different storage implementations
/// without creating circular dependencies
pub trait PredicatesDatabaseAccess {
    /// Get all active Stacks predicates from storage
    fn get_active_stacks_predicates(
        &self,
        ctx: &Context,
    ) -> Result<Vec<(StacksChainhookInstance, PredicateStatus)>, String>;

    /// Get all active Bitcoin predicates from storage
    fn get_active_bitcoin_predicates(
        &self,
        ctx: &Context,
    ) -> Result<Vec<(BitcoinChainhookInstance, PredicateStatus)>, String>;
}

impl PredicatesDatabaseAccess for () {
    fn get_active_stacks_predicates(
        &self,
        _ctx: &Context,
    ) -> Result<Vec<(StacksChainhookInstance, PredicateStatus)>, String> {
        Ok(vec![])
    }

    fn get_active_bitcoin_predicates(
        &self,
        _ctx: &Context,
    ) -> Result<Vec<(BitcoinChainhookInstance, PredicateStatus)>, String> {
        Ok(vec![])
    }
}
