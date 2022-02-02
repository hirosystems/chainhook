pub mod annotation;
pub mod ast_visitor;
pub mod call_checker;
pub mod check_checker;
pub mod contract_call_detector;

use serde::Deserialize;

use crate::analysis::annotation::Annotation;
use crate::clarity::analysis::analysis_db::AnalysisDatabase;
use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::diagnostic::Diagnostic;

use self::call_checker::CallChecker;
use self::check_checker::CheckChecker;
use self::contract_call_detector::ContractCallDetector;

pub type AnalysisResult = Result<Vec<Diagnostic>, Vec<Diagnostic>>;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct AnalysisSettings {
    check_checker: check_checker::Settings,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct AnalysisSettingsFile {
    check_checker: Option<check_checker::SettingsFile>,
}

impl From<AnalysisSettingsFile> for AnalysisSettings {
    fn from(from_file: AnalysisSettingsFile) -> Self {
        if let Some(checker_settings) = from_file.check_checker {
            AnalysisSettings {
                check_checker: check_checker::Settings::from(checker_settings),
            }
        } else {
            AnalysisSettings::default()
        }
    }
}

pub trait AnalysisPass {
    fn run_pass(
        contract_analysis: &mut ContractAnalysis,
        analysis_db: &mut AnalysisDatabase,
        annotations: &Vec<Annotation>,
        settings: AnalysisSettings,
    ) -> AnalysisResult;
}

pub fn run_analysis(
    contract_analysis: &mut ContractAnalysis,
    analysis_db: &mut AnalysisDatabase,
    pass_list: &Vec<String>,
    annotations: &Vec<Annotation>,
    settings: AnalysisSettings,
) -> AnalysisResult {
    let mut errors: Vec<Diagnostic> = Vec::new();
    let mut passes: Vec<
        fn(
            &mut ContractAnalysis,
            &mut AnalysisDatabase,
            &Vec<Annotation>,
            settings: AnalysisSettings,
        ) -> AnalysisResult,
    > = vec![ContractCallDetector::run_pass, CallChecker::run_pass];
    for pass in pass_list {
        match pass.as_str() {
            "all" => passes.append(&mut vec![CheckChecker::run_pass]),
            "check_checker" => passes.push(CheckChecker::run_pass),
            _ => panic!("{}: Unrecognized analysis pass: {}", red!("error"), pass),
        }
    }

    for pass in passes {
        // Collect warnings and continue, or if there is an error, return.
        match pass(contract_analysis, analysis_db, annotations, settings) {
            Ok(mut w) => errors.append(&mut w),
            Err(mut e) => {
                errors.append(&mut e);
                return Err(errors);
            }
        }
    }

    Ok(errors)
}
