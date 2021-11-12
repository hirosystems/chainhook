pub mod ast_visitor;
pub mod contract_call_detector;
pub mod taint_checker;

use crate::clarity::analysis::analysis_db::AnalysisDatabase;
use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::diagnostic::Diagnostic;

use self::contract_call_detector::ContractCallDetector;
use self::taint_checker::TaintChecker;

pub type AnalysisResult = Result<Vec<Diagnostic>, Vec<Diagnostic>>;

pub trait AnalysisPass {
    fn run_pass(
        contract_analysis: &mut ContractAnalysis,
        analysis_db: &mut AnalysisDatabase,
    ) -> AnalysisResult;
}

pub fn run_analysis(
    contract_analysis: &mut ContractAnalysis,
    analysis_db: &mut AnalysisDatabase,
) -> AnalysisResult {
    let mut errors: Vec<Diagnostic> = Vec::new();
    let passes = [ContractCallDetector::run_pass, TaintChecker::run_pass];
    for pass in passes {
        // Collect warnings and continue, or if there is an error, return.
        match pass(contract_analysis, analysis_db) {
            Ok(mut w) => errors.append(&mut w),
            Err(mut e) => {
                errors.append(&mut e);
                return Err(errors);
            }
        }
    }

    Ok(errors)
}
