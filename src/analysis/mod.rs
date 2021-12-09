pub mod ast_visitor;
pub mod check_checker;
pub mod contract_call_detector;

use crate::clarity::analysis::analysis_db::AnalysisDatabase;
use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::diagnostic::Diagnostic;

use self::check_checker::CheckChecker;
use self::contract_call_detector::ContractCallDetector;

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
    pass_list: &Vec<String>,
) -> AnalysisResult {
    let mut errors: Vec<Diagnostic> = Vec::new();
    let mut passes: Vec<fn(&mut ContractAnalysis, &mut AnalysisDatabase) -> AnalysisResult> =
        vec![ContractCallDetector::run_pass];
    for pass in pass_list {
        match pass.as_str() {
            "all" => passes.append(&mut vec![CheckChecker::run_pass]),
            "check_checker" => passes.push(CheckChecker::run_pass),
            _ => panic!("{}: Unrecognized analysis pass: {}", red!("error"), pass),
        }
    }

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
