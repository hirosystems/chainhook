use crate::chainhooks::bitcoin::BitcoinChainhookInstance;
use crate::chainhooks::stacks::StacksChainhookInstance;
use crate::chainhooks::types::{ChainhookInstance, PredicateStatus};
use crate::observer::PredicateEvaluationReport;
use crate::utils::Context;

/// Trait for predicate storage operations needed by the SDK
/// This allows the SDK to work with different storage implementations
/// without creating circular dependencies
pub trait PredicatesDatabaseAccess {
    fn get_predicate(&self, uuid: &String, ctx: &Context) -> Result<Option<(ChainhookInstance, PredicateStatus)>, String>;

    fn insert_predicate(&self, predicate: ChainhookInstance, ctx: &Context) -> Result<(), String>;

    fn enable_predicate(&self, predicate: ChainhookInstance, ctx: &Context) -> Result<(), String>;

    fn delete_predicate(&self, uuid: &String, ctx: &Context) -> Result<(), String>;

    fn expire_stacks_predicates_for_block(&self, block_height: u64, ctx: &Context) -> Result<(), String>;

    fn expire_bitcoin_predicates_for_block(&self, block_height: u64, ctx: &Context) -> Result<(), String>;

    fn interrupt_predicate(&self, uuid: &String, error: String, ctx: &Context) -> Result<(), String>;

    fn update_stacks_predicates_from_report(&self, report: PredicateEvaluationReport, ctx: &Context) -> Result<(), String>;

    fn update_bitcoin_predicates_from_report(&self, report: PredicateEvaluationReport, ctx: &Context) -> Result<(), String>;

    fn get_all_predicates(&self, ctx: &Context) -> Result<Vec<(ChainhookInstance, PredicateStatus)>, String>;

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
    fn get_predicate(&self, _uuid: &String, _ctx: &Context) -> Result<Option<(ChainhookInstance, PredicateStatus)>, String> {
        Ok(None)
    }

    fn insert_predicate(
        &self,
        _predicate: ChainhookInstance,
        _ctx: &Context,
    ) -> Result<(), String> {
        Ok(())
    }

    fn enable_predicate(
        &self,
        _predicate: ChainhookInstance,
        _ctx: &Context,
    ) -> Result<(), String> {
        Ok(())
    }

    fn delete_predicate(&self, _uuid: &String, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn expire_stacks_predicates_for_block(&self, _block_height: u64, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn expire_bitcoin_predicates_for_block(&self, _block_height: u64, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn interrupt_predicate(&self, _uuid: &String, _error: String, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn update_stacks_predicates_from_report(&self, _report: PredicateEvaluationReport, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn update_bitcoin_predicates_from_report(&self, _report: PredicateEvaluationReport, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn get_all_predicates(&self, _ctx: &Context) -> Result<Vec<(ChainhookInstance, PredicateStatus)>, String> {
        Ok(vec![])
    }

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
