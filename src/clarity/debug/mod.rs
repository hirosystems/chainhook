use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt::Display;

use crate::clarity::contexts::{Environment, LocalContext};
use crate::clarity::database::ClarityDatabase;
use crate::clarity::diagnostic::Level;
use crate::clarity::errors::Error;
use crate::clarity::eval;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::representations::Span;
use crate::clarity::representations::SymbolicExpression;
use crate::clarity::types::QualifiedContractIdentifier;
use crate::clarity::types::Value;
use crate::clarity::{ContractName, SymbolicExpressionType};
use crate::repl::ast::build_ast;

use super::EvalHook;

pub mod cli;
pub mod dap;

#[derive(Clone)]
pub struct Source {
    name: QualifiedContractIdentifier,
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub struct Breakpoint {
    id: usize,
    verified: bool,
    data: BreakpointData,
    source: Source,
    span: Option<Span>,
}

impl Display for Breakpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}{}", self.id, self.source, self.data)
    }
}

pub enum BreakpointData {
    Source(SourceBreakpoint),
    Function(FunctionBreakpoint),
    Data(DataBreakpoint),
}

impl Display for BreakpointData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakpointData::Source(source) => write!(f, "{}", source),
            BreakpointData::Function(function) => write!(f, "{}", function),
            BreakpointData::Data(data) => write!(f, "{}", data),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct SourceBreakpoint {
    line: u32,
    column: Option<u32>,
}

impl Display for SourceBreakpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let column = if let Some(column) = self.column {
            format!(":{}", column)
        } else {
            String::new()
        };
        write!(f, ":{}{}", self.line, column)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AccessType {
    Read,
    Write,
    ReadWrite,
}

impl Display for AccessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccessType::Read => write!(f, "(r)"),
            AccessType::Write => write!(f, "(w)"),
            AccessType::ReadWrite => write!(f, "(rw)"),
        }
    }
}

pub struct DataBreakpoint {
    name: String,
    access_type: AccessType,
}

impl Display for DataBreakpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".{} {}", self.name, self.access_type)
    }
}

pub struct FunctionBreakpoint {
    name: String,
}

impl Display for FunctionBreakpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".{}", self.name)
    }
}

#[derive(PartialEq, Debug, Clone)]
pub(crate) enum State {
    Start,
    Continue,
    StepOver(u64),
    StepIn,
    Finish(u64),
    Finished,
    Break(usize),
    DataBreak(usize, AccessType),
    Pause,
    Quit,
}

struct ExprState {
    id: u64,
    active_breakpoints: Vec<usize>,
}

impl ExprState {
    pub fn new(id: u64) -> ExprState {
        ExprState {
            id,
            active_breakpoints: Vec::new(),
        }
    }
}

impl PartialEq for ExprState {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

pub struct DebugState {
    breakpoints: BTreeMap<usize, Breakpoint>,
    watchpoints: BTreeMap<usize, Breakpoint>,
    break_locations: HashMap<QualifiedContractIdentifier, HashSet<usize>>,
    watch_variables: HashMap<(QualifiedContractIdentifier, String), HashSet<usize>>,
    active_breakpoints: HashSet<usize>,
    state: State,
    stack: Vec<ExprState>,
    unique_id: usize,
    debug_cmd_contract: QualifiedContractIdentifier,
    debug_cmd_source: String,
}

impl DebugState {
    pub fn new(contract_id: &QualifiedContractIdentifier, snippet: &str) -> DebugState {
        DebugState {
            breakpoints: BTreeMap::new(),
            watchpoints: BTreeMap::new(),
            break_locations: HashMap::new(),
            watch_variables: HashMap::new(),
            active_breakpoints: HashSet::new(),
            state: State::Start,
            stack: Vec::new(),
            unique_id: 0,
            debug_cmd_contract: contract_id.clone(),
            debug_cmd_source: snippet.to_string(),
        }
    }

    fn get_unique_id(&mut self) -> usize {
        self.unique_id += 1;
        self.unique_id
    }

    fn continue_execution(&mut self) {
        self.state = State::Continue;
    }

    fn step_over(&mut self, id: u64) {
        self.state = State::StepOver(id);
    }

    fn step_in(&mut self) {
        self.state = State::StepIn;
    }

    fn finish(&mut self) {
        if self.stack.len() >= 2 {
            self.state = State::Finish(self.stack[self.stack.len() - 2].id);
        } else {
            self.state = State::Continue;
        }
    }

    fn quit(&mut self) {
        self.state = State::Quit;
    }

    fn add_breakpoint(&mut self, mut breakpoint: Breakpoint) -> usize {
        let id = self.get_unique_id();
        breakpoint.id = id;

        if let Some(set) = self.break_locations.get_mut(&breakpoint.source.name) {
            set.insert(breakpoint.id);
        } else {
            let mut set = HashSet::new();
            set.insert(id);
            self.break_locations
                .insert(breakpoint.source.name.clone(), set);
        }

        self.breakpoints.insert(id, breakpoint);
        id
    }

    fn delete_all_breakpoints(&mut self) {
        for (id, breakpoint) in &self.breakpoints {
            let set = self
                .break_locations
                .get_mut(&breakpoint.source.name)
                .unwrap();
            set.remove(&breakpoint.id);
        }
        self.breakpoints.clear();
    }

    fn delete_breakpoint(&mut self, id: usize) -> bool {
        if let Some(breakpoint) = self.breakpoints.remove(&id) {
            let set = self
                .break_locations
                .get_mut(&breakpoint.source.name)
                .unwrap();
            set.remove(&breakpoint.id);
            true
        } else {
            false
        }
    }

    fn add_watchpoint(&mut self, mut breakpoint: Breakpoint) {
        breakpoint.id = self.get_unique_id();
        let name = match &breakpoint.data {
            BreakpointData::Data(data) => data.name.clone(),
            _ => panic!("called add_watchpoint with non-data breakpoint"),
        };

        let key = (breakpoint.source.name.clone(), name);
        if let Some(set) = self.watch_variables.get_mut(&key) {
            set.insert(breakpoint.id);
        } else {
            let mut set = HashSet::new();
            set.insert(breakpoint.id);
            self.watch_variables.insert(key, set);
        }

        self.watchpoints.insert(breakpoint.id, breakpoint);
    }

    fn delete_all_watchpoints(&mut self) {
        for (id, breakpoint) in &self.watchpoints {
            let name = match &breakpoint.data {
                BreakpointData::Data(data) => data.name.clone(),
                _ => continue,
            };
            let set = self
                .watch_variables
                .get_mut(&(breakpoint.source.name.clone(), name))
                .unwrap();
            set.remove(&breakpoint.id);
        }
        self.watchpoints.clear();
    }

    fn delete_watchpoint(&mut self, id: usize) -> bool {
        if let Some(breakpoint) = self.watchpoints.remove(&id) {
            let name = match breakpoint.data {
                BreakpointData::Data(data) => data.name,
                _ => panic!("called delete_watchpoint with non-data breakpoint"),
            };
            let set = self
                .watch_variables
                .get_mut(&(breakpoint.source.name, name))
                .unwrap();
            set.remove(&breakpoint.id);
            true
        } else {
            false
        }
    }

    fn pause(&mut self) {
        self.state = State::Pause;
    }

    fn did_hit_source_breakpoint(
        &self,
        contract_id: &QualifiedContractIdentifier,
        span: &Span,
    ) -> Option<usize> {
        if let Some(set) = self.break_locations.get(contract_id) {
            for id in set {
                // Don't break in a subexpression of an expression which has
                // already triggered this breakpoint
                if self.active_breakpoints.contains(id) {
                    continue;
                }

                let breakpoint = match self.breakpoints.get(id) {
                    Some(breakpoint) => breakpoint,
                    None => panic!("internal error: breakpoint {} not found", id),
                };

                if let Some(break_span) = &breakpoint.span {
                    if break_span.start_line == span.start_line
                        && (break_span.start_column == 0
                            || break_span.start_column == span.start_column)
                    {
                        return Some(breakpoint.id);
                    }
                }
            }
        }
        None
    }

    fn did_hit_data_breakpoint(
        &self,
        contract_id: &QualifiedContractIdentifier,
        expr: &SymbolicExpression,
    ) -> Option<(usize, AccessType)> {
        match &expr.expr {
            SymbolicExpressionType::List(list) => {
                // Check if we hit a data breakpoint
                if let Some((function_name, args)) = list.split_first() {
                    if let Some(function_name) = function_name.match_atom() {
                        if let Some(native_function) =
                            NativeFunctions::lookup_by_name(function_name)
                        {
                            use crate::clarity::functions::NativeFunctions::*;
                            if let Some((name, access_type)) = match native_function {
                                FetchVar => Some((
                                    args[0].match_atom().unwrap().to_string(),
                                    AccessType::Read,
                                )),
                                SetVar => Some((
                                    args[0].match_atom().unwrap().to_string(),
                                    AccessType::Write,
                                )),
                                FetchEntry => Some((
                                    args[0].match_atom().unwrap().to_string(),
                                    AccessType::Read,
                                )),
                                SetEntry => Some((
                                    args[0].match_atom().unwrap().to_string(),
                                    AccessType::Write,
                                )),
                                InsertEntry => Some((
                                    args[0].match_atom().unwrap().to_string(),
                                    AccessType::Write,
                                )),
                                DeleteEntry => Some((
                                    args[0].match_atom().unwrap().to_string(),
                                    AccessType::Write,
                                )),
                                _ => None,
                            } {
                                let key = (contract_id.clone(), name);
                                if let Some(set) = self.watch_variables.get(&key) {
                                    for id in set {
                                        let watchpoint = match self.watchpoints.get(id) {
                                            Some(watchpoint) => watchpoint,
                                            None => panic!(
                                                "internal error: watchpoint {} not found",
                                                id
                                            ),
                                        };

                                        if let BreakpointData::Data(data) = &watchpoint.data {
                                            match (data.access_type, access_type) {
                                                (AccessType::Read, AccessType::Read)
                                                | (AccessType::Write, AccessType::Write)
                                                | (AccessType::ReadWrite, AccessType::Read)
                                                | (AccessType::ReadWrite, AccessType::Write) => {
                                                    return Some((watchpoint.id, access_type))
                                                }
                                                _ => (),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    // Returns a bool which indicates if execution should resume (true) or if
    // it should wait for input (false).
    fn begin_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
    ) -> bool {
        self.stack.push(ExprState::new(expr.id));

        // If user quit debug session, we can't stop executing, but don't do anything else.
        if self.state == State::Quit {
            return true;
        }

        // Check if we have hit a source breakpoint
        if let Some(breakpoint) =
            self.did_hit_source_breakpoint(&env.contract_context.contract_identifier, &expr.span)
        {
            self.active_breakpoints.insert(breakpoint);
            let top = self.stack.last_mut().unwrap();
            top.active_breakpoints.push(breakpoint);

            self.state = State::Break(breakpoint);
        }

        // Always skip over non-list expressions (values).
        match expr.expr {
            SymbolicExpressionType::List(_) => (),
            _ => return true,
        };

        if let Some((watchpoint, access_type)) =
            self.did_hit_data_breakpoint(&env.contract_context.contract_identifier, expr)
        {
            self.state = State::DataBreak(watchpoint, access_type);
        }

        match self.state {
            State::Continue | State::Quit | State::Finish(_) => return true,
            State::StepOver(step_over_id) => {
                if self
                    .stack
                    .iter()
                    .find(|&state| state.id == step_over_id)
                    .is_some()
                {
                    // We're still inside the expression which should be stepped over,
                    // so return to execution.
                    return true;
                }
            }
            State::Start
            | State::StepIn
            | State::Break(_)
            | State::DataBreak(..)
            | State::Pause
            | State::Finished => (),
        };

        false
    }

    // Returns a bool which indicates if the result should be printed (finish)
    fn finish_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
        res: &Result<Value, Error>,
    ) -> bool {
        let state = self.stack.pop().unwrap();
        assert_eq!(state.id, expr.id);

        // Remove any active breakpoints for this expression
        for breakpoint in state.active_breakpoints {
            self.active_breakpoints.remove(&breakpoint);
        }

        // Only print the returned value if this resolves a finish command
        match self.state {
            State::Finish(finish_id) if finish_id == state.id => {
                self.state = State::Finished;
                true
            }
            _ => false,
        }
    }
}
