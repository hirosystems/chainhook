use crate::clarity::analysis::AnalysisDatabase;
use crate::clarity::types::{QualifiedContractIdentifier, PrincipalData};
use crate::clarity::{ast, analysis};
use crate::clarity::ast::ContractAST;
use crate::clarity::analysis::ContractAnalysis;
use crate::clarity::costs::LimitedCostTracker;
use crate::clarity::database::{Datastore, NULL_HEADER_DB};
use crate::clarity::contexts::{OwnedEnvironment, ContractContext, GlobalContext};
use crate::clarity::contracts::Contract;
use crate::clarity::eval_all;
use crate::clarity::diagnostic::Diagnostic;
use crate::clarity::util::{StacksAddress};

#[derive(Clone, Debug)]
pub struct ClarityInterpreter {
    datastore: Datastore
}

impl ClarityInterpreter {

    pub fn new() -> ClarityInterpreter {
        let datastore = Datastore::new();

        ClarityInterpreter {
            datastore
        }
    }

    pub fn run(&mut self, snippet: String, contract_identifier: QualifiedContractIdentifier) -> Result<(bool, String), (String, Option<Diagnostic>)> {

        let mut ast = self.build_ast(contract_identifier.clone(), snippet.clone())?;
        let analysis = self.run_analysis(contract_identifier.clone(), &mut ast)?;
        let result = self.execute(contract_identifier, &mut ast, snippet, analysis)?;

        // todo: instead of just returning the value, we should be returning:
        // - value
        // - execution cost
        // - events emitted
        Ok(result)
    }

    pub fn build_ast(&mut self, contract_identifier: QualifiedContractIdentifier, snippet: String) -> Result<ContractAST, (String, Option<Diagnostic>)> {
        let contract_ast = match ast::build_ast(&contract_identifier, &snippet, &mut ()) {
            Ok(res) => res,
            Err(error) => {
                let message = format!("Parsing error: {}", error.diagnostic.message);
                return Err((message, Some(error.diagnostic)));
            }
        };
        Ok(contract_ast)
    }

    pub fn run_analysis(&mut self, contract_identifier: QualifiedContractIdentifier, contract_ast: &mut ContractAST) -> Result<ContractAnalysis, (String, Option<Diagnostic>)> {

        let mut analysis_db = AnalysisDatabase::new(&mut self.datastore);

        let contract_analysis = match analysis::run_analysis(
            &contract_identifier, 
            &mut contract_ast.expressions,
            &mut analysis_db, 
            false,
            LimitedCostTracker::new_max_limit()) 
        {
            Ok(res) => res,
            Err((error, cost_tracker)) => {
                let message = format!("Analysis error: {}", error.diagnostic.message);
                return Err((message, Some(error.diagnostic)));
            }
        };
        Ok(contract_analysis)
    }

    pub fn execute(&mut self, contract_identifier: QualifiedContractIdentifier, contract_ast: &mut ContractAST, snippet: String, contract_analysis: ContractAnalysis) -> Result<(bool, String), (String, Option<Diagnostic>)> {
        let mut contract_context = ContractContext::new(contract_identifier.clone());
        let conn = self.datastore.as_clarity_db(&NULL_HEADER_DB);

        let mut global_context = GlobalContext::new(conn, LimitedCostTracker::new_max_limit());
        global_context.begin();

        let result = global_context.execute(|g| {
            eval_all(&contract_ast.expressions, &mut contract_context, g)
        });

        let value = match result {
            Ok(Some(value)) => format!("{}", value),
            Ok(None) => format!("()"),
            Err(error) => {
                let error = format!("Runtime Error: {:?}", error);
                return Err((error, None));
            }
        };

        let contract_saved = contract_context.functions.len() > 0 || contract_context.defined_traits.len() > 0;

        let mut contract_synopsis = vec![];

        if contract_saved {
            for (name, defined_func) in contract_context.functions.iter() {
                if !defined_func.is_public() {
                    continue;
                }

                let args: Vec<_> = defined_func.arguments.iter()
                    .zip(defined_func.arg_types.iter())
                    .map(|(n, t)| format!("({} {})", n.as_str(), t))
                    .collect();

                let func_sig = format!("({} {})", name.as_str(), args.join(" "));

                contract_synopsis.push(func_sig);
            }

            for defined_trait in contract_context.defined_traits.iter() {

            }

            global_context.database.insert_contract_hash(&contract_identifier, &snippet).unwrap();
            let contract = Contract { contract_context };
            global_context.database.insert_contract(&contract_identifier, contract);
            global_context.database.set_contract_data_size(
                &contract_identifier, 0).unwrap();
        }
        global_context.commit().unwrap();

        if !contract_saved {
            return Ok((false, format!("{}", value)))
        }

        let mut analysis_db = AnalysisDatabase::new(&mut self.datastore);
        analysis_db.begin();
        analysis_db.insert_contract(&contract_identifier, &contract_analysis).unwrap();
        analysis_db.commit();
        
        Ok((true, contract_synopsis.join("\n")))
    }

    pub fn credit_stx_balance(&mut self, recipient: PrincipalData, amount: u64) -> Result<String, String> {
        let conn = self.datastore.as_clarity_db(&NULL_HEADER_DB);

        let mut global_context = GlobalContext::new(conn, LimitedCostTracker::new_max_limit());
        global_context.begin();
        let cur_balance = global_context.database.get_account_stx_balance(&recipient);
        let final_balance = cur_balance.checked_add(amount as u128).expect("FATAL: account balance overflow");
        global_context.database.set_account_stx_balance(&recipient, final_balance as u128);

        global_context.commit().unwrap();

        Ok(format!("→ {}: {} µSTX", recipient, final_balance))
    }
}
