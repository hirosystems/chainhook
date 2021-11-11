pub mod contract_call_detector;

use crate::clarity::analysis::analysis_db::AnalysisDatabase;
use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::diagnostic::Diagnostic;

use self::contract_call_detector::ContractCallDetector;

pub type AnalysisResult = Result<(), Vec<Diagnostic>>;

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
    let passes = [ContractCallDetector::run_pass];
    for pass in passes {
        match pass(contract_analysis, analysis_db) {
            Ok(_) => (),
            Err(mut e) => errors.append(&mut e),
        }
    }

    if errors.len() == 0 {
        Ok(())
    } else {
        Err(errors)
    }
}
