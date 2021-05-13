use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use crate::clarity::{analysis::AnalysisDatabase, database::ClarityBackingStore};
use crate::clarity::analysis::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::contexts::{ContractContext, GlobalContext};
use crate::clarity::contracts::Contract;
use crate::clarity::costs::{LimitedCostTracker, ExecutionCost};
use crate::clarity::database::{Datastore, NULL_HEADER_DB};
use crate::clarity::diagnostic::Diagnostic;
use crate::clarity::eval_all;
use crate::clarity::types::{self, PrincipalData, StandardPrincipalData, QualifiedContractIdentifier};
use crate::clarity::util::StacksAddress;
use crate::clarity::{analysis, ast};
use crate::clarity::events::*;
use crate::repl::{CostSynthesis, ExecutionResult};
use serde_json::Value;

pub const BLOCK_LIMIT_MAINNET: ExecutionCost = ExecutionCost {
    write_length: 15_000_000,
    write_count: 7_750,
    read_length: 100_000_000,
    read_count: 7_750,
    runtime: 5_000_000_000,
};

#[derive(Clone, Debug)]
pub struct ClarityInterpreter {
    pub datastore: Datastore,
    tx_sender: StandardPrincipalData,
    accounts: BTreeSet<String>,
    tokens: BTreeMap<String, BTreeMap<String, u128>>,
}

impl ClarityInterpreter {
    pub fn new(tx_sender: StandardPrincipalData) -> ClarityInterpreter {
        let datastore = Datastore::new();
        let accounts = BTreeSet::new();
        let tokens = BTreeMap::new();
        ClarityInterpreter { datastore, tx_sender, accounts, tokens }
    }

    pub fn run(
        &mut self,
        snippet: String,
        contract_identifier: QualifiedContractIdentifier,
        cost_track: bool
    ) -> Result<ExecutionResult, (String, Option<Diagnostic>)> {
        let mut ast = self.build_ast(contract_identifier.clone(), snippet.clone())?;
        let analysis = self.run_analysis(contract_identifier.clone(), &mut ast)?;
        let result = self.execute(contract_identifier, &mut ast, snippet, analysis, cost_track)?;

        // todo: instead of just returning the value, we should be returning:
        // - value
        // - execution cost
        // - events emitted
        Ok(result)
    }

    pub fn detect_dependencies(
        &self,
        contract_id: String,
        snippet: String
    ) -> Result<BTreeSet<String>, String> {
        let contract_id = QualifiedContractIdentifier::parse(&contract_id)
            .unwrap();
        let ast = match self.build_ast(contract_id, snippet.clone()) {
            Err(e) => return Err(format!("{:?}", e)),
            Ok(ast) => ast,
        };

        let deps = ContractCallDetector::run_pass(&ast);
        Ok(deps)
    }

    pub fn build_ast(
        &self,
        contract_identifier: QualifiedContractIdentifier,
        snippet: String,
    ) -> Result<ContractAST, (String, Option<Diagnostic>)> {
        let contract_ast = match ast::build_ast(&contract_identifier, &snippet, &mut ()) {
            Ok(res) => res,
            Err(error) => {
                let message = format!("Parsing error: {}", error.diagnostic.message);
                return Err((message, Some(error.diagnostic)));
            }
        };
        Ok(contract_ast)
    }

    pub fn run_analysis(
        &mut self,
        contract_identifier: QualifiedContractIdentifier,
        contract_ast: &mut ContractAST,
    ) -> Result<ContractAnalysis, (String, Option<Diagnostic>)> {
        let mut analysis_db = AnalysisDatabase::new(&mut self.datastore);

        let contract_analysis = match analysis::run_analysis(
            &contract_identifier,
            &mut contract_ast.expressions,
            &mut analysis_db,
            false,
            LimitedCostTracker::new_free(),
        ) {
            Ok(res) => res,
            Err((error, cost_tracker)) => {
                let message = format!("Analysis error: {}", error.diagnostic.message);
                return Err((message, Some(error.diagnostic)));
            }
        };
        Ok(contract_analysis)
    }

    #[allow(unused_assignments)]
    pub fn execute(
        &mut self,
        contract_identifier: QualifiedContractIdentifier,
        contract_ast: &mut ContractAST,
        snippet: String,
        contract_analysis: ContractAnalysis,
        cost_track: bool,
    ) -> Result<ExecutionResult, (String, Option<Diagnostic>)> {

        let mut execution_result = ExecutionResult::default();
        let mut contract_saved = false;
        let mut serialized_events = vec![];
        let mut accounts_to_debit = vec![];
        let mut accounts_to_credit = vec![];
        let mut contract_context = ContractContext::new(contract_identifier.clone());
        let value = {
            let mut conn = self.datastore.as_clarity_db(&NULL_HEADER_DB);
            let cost_tracker = if cost_track {
                LimitedCostTracker::new(false, BLOCK_LIMIT_MAINNET.clone(), &mut conn).unwrap()
            } else {
                LimitedCostTracker::new_free()
            };
            let mut global_context = GlobalContext::new(false, conn, cost_tracker);
            global_context.begin();

            let result = global_context
                .execute(|g| eval_all(&contract_ast.expressions, &mut contract_context, g));

            let value = match result {
                Ok(Some(value)) => format!("{}", value),
                Ok(None) => format!("()"),
                Err(error) => {
                    let error = format!("Runtime Error: {:?}", error);
                    return Err((error, None));
                }
            };

            if cost_track {
                execution_result.cost = Some(CostSynthesis::from_cost_tracker(&global_context.cost_track));
            }
            
            let mut emitted_events = global_context.event_batches
                .iter()
                .flat_map(|b| b.events.clone())
                .collect::<Vec<_>>();

            for event in emitted_events.drain(..) {
                match event {
                    StacksTransactionEvent::STXEvent(STXEventType::STXTransferEvent(ref event_data)) => {
                        accounts_to_debit.push((event_data.sender.to_string(), "STX".to_string(), event_data.amount.clone()));
                        accounts_to_credit.push((event_data.recipient.to_string(), "STX".to_string(), event_data.amount.clone()));
                    },
                    StacksTransactionEvent::STXEvent(STXEventType::STXMintEvent(ref event_data)) => {
                        accounts_to_credit.push((event_data.recipient.to_string(), "STX".to_string(), event_data.amount.clone()));
                    },
                    StacksTransactionEvent::STXEvent(STXEventType::STXBurnEvent(ref event_data)) => {
                        accounts_to_debit.push((event_data.sender.to_string(), "STX".to_string(), event_data.amount.clone()));
                    },
                    StacksTransactionEvent::FTEvent(FTEventType::FTTransferEvent(ref event_data)) => {
                        accounts_to_credit.push((event_data.recipient.to_string(), event_data.asset_identifier.sugared(), event_data.amount.clone()));
                        accounts_to_debit.push((event_data.sender.to_string(), event_data.asset_identifier.sugared(), event_data.amount.clone()));
                    },
                    StacksTransactionEvent::FTEvent(FTEventType::FTMintEvent(ref event_data)) => {
                        accounts_to_credit.push((event_data.recipient.to_string(), event_data.asset_identifier.sugared(), event_data.amount.clone()));
                    },
                    StacksTransactionEvent::FTEvent(FTEventType::FTBurnEvent(ref event_data)) => {
                        accounts_to_debit.push((event_data.sender.to_string(), event_data.asset_identifier.sugared(), event_data.amount.clone()));
                    },
                    StacksTransactionEvent::NFTEvent(NFTEventType::NFTTransferEvent(ref event_data)) => {
                        accounts_to_credit.push((event_data.recipient.to_string(), event_data.asset_identifier.sugared(), 1));
                        accounts_to_debit.push((event_data.sender.to_string(), event_data.asset_identifier.sugared(), 1));
                    },
                    StacksTransactionEvent::NFTEvent(NFTEventType::NFTMintEvent(ref event_data)) => {
                        accounts_to_debit.push((event_data.recipient.to_string(), event_data.asset_identifier.sugared(), 1));
                    },
                    StacksTransactionEvent::NFTEvent(NFTEventType::NFTBurnEvent(ref event_data)) => {
                        accounts_to_debit.push((event_data.sender.to_string(), event_data.asset_identifier.sugared(), 1));
                    },
                    // StacksTransactionEvent::SmartContractEvent(event_data) => ,
                    // StacksTransactionEvent::STXEvent(STXEventType::STXLockEvent(event_data)) => ,
                    _ => {}
                };

                serialized_events.push(event.json_serialize());
            }

            contract_saved =
                contract_context.functions.len() > 0 || contract_context.defined_traits.len() > 0;

            if contract_saved {
                let mut functions = BTreeMap::new();
                for (name, defined_func) in contract_context.functions.iter() {
                    if !defined_func.is_public() {
                        continue;
                    }

                    let args: Vec<_> = defined_func
                        .arguments
                        .iter()
                        .zip(defined_func.arg_types.iter())
                        .map(|(n, t)| format!("({} {})", n.as_str(), t))
                        .collect();

                    functions.insert(name.to_string(), args);
                }
                execution_result.contract = Some((format!("{}", contract_identifier), functions));

                for defined_trait in contract_context.defined_traits.iter() {}

                global_context
                    .database
                    .insert_contract_hash(&contract_identifier, &snippet)
                    .unwrap();
                let contract = Contract { contract_context };
                global_context
                    .database
                    .insert_contract(&contract_identifier, contract);
                global_context
                    .database
                    .set_contract_data_size(&contract_identifier, 0)
                    .unwrap();
            }
            global_context.commit().unwrap();
            value
        };

        execution_result.events = serialized_events;

        for (account, token, value) in accounts_to_credit.drain(..) {
            self.credit_token(account, token, value);
        }

        for (account, token, value) in accounts_to_debit.drain(..) {
            self.debit_token(account, token, value);
        }

        if !contract_saved {
            execution_result.result = Some(format!("{}", value));
            return Ok(execution_result);
        }

        let mut analysis_db = AnalysisDatabase::new(&mut self.datastore);
        analysis_db.begin();
        analysis_db
            .insert_contract(&contract_identifier, &contract_analysis)
            .unwrap();
        analysis_db.commit();

        Ok(execution_result)
    }

    pub fn credit_stx_balance(
        &mut self,
        recipient: PrincipalData,
        amount: u64,
    ) -> Result<String, String> {
        let final_balance = {
            let conn = self.datastore.as_clarity_db(&NULL_HEADER_DB);
            let mut global_context = GlobalContext::new(false, conn, LimitedCostTracker::new_free());
            global_context.begin();
            let mut cur_balance = global_context.database.get_stx_balance_snapshot(&recipient);
            cur_balance.credit(amount as u128);
            let final_balance = cur_balance.get_available_balance();
            cur_balance.save();
            global_context.commit().unwrap();
            final_balance
        };
        self.credit_token(recipient.to_string(), "STX".to_string(), amount.into());
        Ok(format!("→ {}: {} µSTX", recipient, final_balance))
    }

    pub fn set_tx_sender(&mut self, tx_sender: StandardPrincipalData) {
        self.tx_sender = tx_sender;
    }

    pub fn get_tx_sender(&self) -> StandardPrincipalData {
        self.tx_sender.clone()
    }

    pub fn advance_chain_tip(&mut self, count: u32) -> u32 {
        self.datastore.advance_chain_tip(count)
    }

    pub fn get_block_height(&mut self) -> u32 {
        self.datastore.get_current_block_height()
    }

    fn credit_token(&mut self, account: String, token: String, value: u128) {
        self.accounts.insert(account.clone());
        match self.tokens.entry(token) {
            Entry::Occupied(balances) => {
                balances.into_mut().entry(account)
                    .and_modify(|e| { *e += value })
                    .or_insert(value);
            }
            Entry::Vacant(v) => {
                let mut balances = BTreeMap::new();
                balances.insert(account, value);
                v.insert(balances);
            }
        };
    }

    fn debit_token(&mut self, account: String, token: String, value: u128) {
        self.accounts.insert(account.clone());
        match self.tokens.entry(token) {
            Entry::Occupied(balances) => {
                balances.into_mut().entry(account)
                    .and_modify(|e| { *e -= value })
                    .or_insert(value);
            }
            Entry::Vacant(v) => {
                let mut balances = BTreeMap::new();
                balances.insert(account, value);
                v.insert(balances);
            }
        };
    }

    pub fn get_assets_maps(&self) -> BTreeMap<String, BTreeMap<String, u128>> {
        self.tokens.clone()
    }

    pub fn get_tokens(&self) -> Vec<String> {
        self.tokens.keys().cloned().collect()
    }

    pub fn get_accounts(&self) -> Vec<String> {
        self.accounts.clone().into_iter().collect::<Vec<_>>()
    }

    pub fn get_balance_for_account(&self, account: &str, token: &str) -> u128 {
        match self.tokens.get(token) {
            Some(balances) => match balances.get(account) {
                Some(value) => value.clone(),
                _ => 0,
            },
            _ => 0,
        }
    }

}

use crate::clarity::representations::SymbolicExpressionType::{
    Atom, AtomValue, Field, List, LiteralValue, TraitReference,
};
use crate::clarity::representations::{
    SymbolicExpression
};

pub fn traverse(exprs: &[SymbolicExpression], deps: &mut BTreeSet<String>) {
    for (i, expression) in exprs.iter().enumerate() {
        if let Some(exprs) = expression.match_list() {
            traverse(exprs, deps);
        } else if let Some(atom) = expression.match_atom() {
            if atom.as_str() == "contract-call?" {
                if let Some(types::Value::Principal(PrincipalData::Contract(ref contract_id))) = exprs[i+1].match_literal_value() {
                    deps.insert(format!("{}", contract_id));
                }
            } else if atom.as_str() == "use-trait" {
                let contract_id = exprs[i+2]
                    .match_field()
                    .unwrap()
                    .clone()
                    .contract_identifier;
                deps.insert(format!("{}", contract_id));
            } else if atom.as_str() == "impl-trait" {
                let contract_id = exprs[i+1]
                    .match_field()
                    .unwrap()
                    .clone()
                    .contract_identifier;
                deps.insert(format!("{}", contract_id));
            }
        };
    }
}

pub struct ContractCallDetector;

impl ContractCallDetector {
    pub fn run_pass(contract_ast: &ContractAST) -> BTreeSet<String> {
        let mut contract_calls = BTreeSet::new();
        traverse(contract_ast.expressions.as_slice(), &mut contract_calls);
        contract_calls
    }
}
