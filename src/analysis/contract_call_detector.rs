use crate::analysis::annotation::Annotation;
use crate::analysis::{AnalysisPass, AnalysisResult, AnalysisSettings};
use crate::clarity::analysis::analysis_db::AnalysisDatabase;
pub use crate::clarity::analysis::types::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::representations::SymbolicExpression;
use crate::clarity::types::{PrincipalData, QualifiedContractIdentifier, Value};
use std::collections::BTreeSet;

pub struct ContractCallDetector;

pub fn traverse(exprs: &[SymbolicExpression], deps: &mut BTreeSet<QualifiedContractIdentifier>) {
    for (i, expression) in exprs.iter().enumerate() {
        if let Some(exprs) = expression.match_list() {
            traverse(exprs, deps);
        } else if let Some(atom) = expression.match_atom() {
            if atom.as_str() == "contract-call?" {
                if let Some(Value::Principal(PrincipalData::Contract(ref contract_id))) =
                    exprs[i + 1].match_literal_value()
                {
                    deps.insert(contract_id.clone());
                }
            } else if atom.as_str() == "use-trait" {
                let contract_id = exprs[i + 2]
                    .match_field()
                    .unwrap()
                    .clone()
                    .contract_identifier;
                deps.insert(contract_id);
            } else if atom.as_str() == "impl-trait" {
                let contract_id = exprs[i + 1]
                    .match_field()
                    .unwrap()
                    .clone()
                    .contract_identifier;
                deps.insert(contract_id);
            }
        };
    }
}

impl AnalysisPass for ContractCallDetector {
    fn run_pass(
        contract_analysis: &mut ContractAnalysis,
        analysis_db: &mut AnalysisDatabase,
        annotations: &Vec<Annotation>,
        settings: AnalysisSettings,
    ) -> AnalysisResult {
        let mut contract_calls = BTreeSet::new();
        traverse(
            contract_analysis.expressions.as_slice(),
            &mut contract_calls,
        );
        for dep in contract_calls.into_iter() {
            contract_analysis.add_dependency(dep);
        }
        Ok(vec![])
    }
}
