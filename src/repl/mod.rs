use crate::clarity::analysis::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::costs::{ExecutionCost, LimitedCostTracker};
use crate::clarity::coverage::TestCoverageReport;
use serde_json::Value;
use std::collections::BTreeMap;

pub mod interpreter;
pub mod session;
pub mod settings;
pub mod command;

pub use interpreter::ClarityInterpreter;
pub use session::Session;
pub use settings::SessionSettings;
pub use command::*;


#[derive(Debug, Copy, Clone)]
pub enum OutputMode {
    Console,
    Json,
}

impl OutputMode {
    pub fn switch(output_mode: &Self) -> Self {
        match output_mode {
            Self::Console => Self::Json,
            Self::Json => Self::Console
        }
    }
}

#[derive(Default, Debug)]
pub struct ExecutionResult {
    pub contract: Option<(
        String,
        String,
        BTreeMap<String, Vec<String>>,
        ContractAST,
        ContractAnalysis,
    )>,
    pub result: Option<String>,
    pub events: Vec<Value>,
    pub cost: Option<CostSynthesis>,
    pub coverage: Option<TestCoverageReport>,
}


#[derive(Clone, Serialize, Debug)]
pub struct CostSynthesis {
    pub total: ExecutionCost,
    pub limit: ExecutionCost,
    pub memory: u64,
    pub memory_limit: u64,
}

impl CostSynthesis {
    pub fn from_cost_tracker(cost_tracker: &LimitedCostTracker) -> CostSynthesis {
        CostSynthesis {
            total: cost_tracker.get_total(),
            limit: cost_tracker.get_limit(),
            memory: cost_tracker.memory,
            memory_limit: cost_tracker.memory_limit,
        }
    }
}
