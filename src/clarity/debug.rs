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
use rustyline::error::ReadlineError;
use rustyline::Editor;

const HISTORY_FILE: Option<&'static str> = option_env!("CLARITY_DEBUG_HISTORY_FILE");

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

pub struct SourceBreakpoint {
    line: u32,
    column: Option<u32>,
    log_message: Option<String>,
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

#[derive(Clone, Copy)]
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

#[derive(PartialEq, Debug)]
enum State {
    Start,
    Continue,
    StepOver(u64),
    StepIn,
    Finish(u64),
    Break,
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
    editor: Editor<()>,
    breakpoints: BTreeMap<usize, Breakpoint>,
    watchpoints: BTreeMap<usize, Breakpoint>,
    break_locations: HashMap<QualifiedContractIdentifier, HashSet<usize>>,
    watch_variables: HashMap<(QualifiedContractIdentifier, String), HashSet<usize>>,
    active_breakpoints: HashSet<usize>,
    state: State,
    stack: Vec<ExprState>,
    unique_id: usize,
}

impl DebugState {
    pub fn new() -> DebugState {
        let mut editor = Editor::<()>::new();
        editor
            .load_history(HISTORY_FILE.unwrap_or(".debug_history"))
            .ok();

        DebugState {
            editor,
            breakpoints: BTreeMap::new(),
            watchpoints: BTreeMap::new(),
            break_locations: HashMap::new(),
            watch_variables: HashMap::new(),
            active_breakpoints: HashSet::new(),
            state: State::Start,
            stack: Vec::new(),
            unique_id: 0,
        }
    }

    fn get_unique_id(&mut self) -> usize {
        self.unique_id += 1;
        self.unique_id
    }

    fn add_breakpoint(&mut self, mut breakpoint: Breakpoint) {
        breakpoint.id = self.get_unique_id();

        if let Some(set) = self.break_locations.get_mut(&breakpoint.source.name) {
            set.insert(breakpoint.id);
        } else {
            let mut set = HashSet::new();
            set.insert(breakpoint.id);
            self.break_locations
                .insert(breakpoint.source.name.clone(), set);
        }

        self.breakpoints.insert(breakpoint.id, breakpoint);
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
    ) -> Option<usize> {
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
                                                    return Some(watchpoint.id)
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

    fn prompt(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
        finish: bool,
    ) {
        let prompt = black!("(debug) ");
        loop {
            let readline = self.editor.readline(&prompt);
            let resume = match readline {
                Ok(mut command) => {
                    if command.is_empty() {
                        match self.editor.history().last() {
                            Some(prev) => command = prev.clone(),
                            None => (),
                        }
                    }
                    self.editor.add_history_entry(&command);
                    self.handle_command(&command, env, context, expr, finish)
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    println!("Use \"q\" or \"quit\" to exit debug mode");
                    false
                }
                Err(err) => {
                    println!("Error: {:?}", err);
                    false
                }
            };

            if resume {
                break;
            }
        }
        self.editor
            .save_history(HISTORY_FILE.unwrap_or(".debug_history"))
            .unwrap();
    }

    // Print the source of the current expr (if it has a valid span).
    fn print_source(&mut self, env: &mut Environment, expr: &SymbolicExpression) {
        let contract_id = &env.contract_context.contract_identifier;
        if expr.span.start_line != 0 {
            match env.global_context.database.get_contract_src(contract_id) {
                Some(contract_source) => {
                    println!(
                        "{}:{}:{}",
                        blue!(format!("{}", contract_id)),
                        expr.span.start_line,
                        expr.span.start_column
                    );
                    let lines: Vec<&str> = contract_source.lines().collect();
                    let first_line = (expr.span.start_line - 1).saturating_sub(3) as usize;
                    let last_line =
                        std::cmp::min(lines.len() - 1, expr.span.start_line as usize + 3);
                    for line in first_line..last_line {
                        if line == (expr.span.start_line as usize - 1) {
                            print!("{}", blue!("-> "));
                        } else {
                            print!("   ");
                        }
                        println!("{} {}", black!(format!("{: <6}", line + 1)), lines[line]);
                        if line == (expr.span.start_line as usize - 1) {
                            println!(
                                "{}",
                                blue!(format!(
                                    "          {: <1$}^",
                                    "",
                                    (expr.span.start_column - 1) as usize
                                ))
                            );
                        }
                    }
                }
                None => {
                    println!("{}", yellow!("source not found"));
                    println!(
                        "{}:{}:{}",
                        contract_id, expr.span.start_line, expr.span.start_column
                    );
                }
            }
        } else {
            println!("{}", yellow!("source information unknown"));
        }
    }

    pub fn begin_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
    ) {
        self.stack.push(ExprState::new(expr.id));

        // If user quit debug session, we can't stop executing, but don't do anything else.
        if self.state == State::Quit {
            return;
        }

        // Check if we have hit a source breakpoint
        if let Some(breakpoint) =
            self.did_hit_source_breakpoint(&env.contract_context.contract_identifier, &expr.span)
        {
            self.active_breakpoints.insert(breakpoint);
            let top = self.stack.last_mut().unwrap();
            top.active_breakpoints.push(breakpoint);

            println!("{} hit breakpoint {}", black!("*"), breakpoint);
            self.state = State::Break;
        }

        // Always skip over non-list expressions (values).
        match expr.expr {
            SymbolicExpressionType::List(_) => (),
            _ => return,
        };

        if let Some(watchpoint) =
            self.did_hit_data_breakpoint(&env.contract_context.contract_identifier, expr)
        {
            println!("{} hit watchpoint {}", black!("*"), watchpoint);
            self.state = State::Break;
        }

        match self.state {
            State::Continue | State::Quit | State::Finish(_) => return,
            State::StepOver(step_over_id) => {
                if self
                    .stack
                    .iter()
                    .find(|&state| state.id == step_over_id)
                    .is_some()
                {
                    // We're still inside the expression which should be stepped over,
                    // so return to execution.
                    return;
                }
            }
            State::Start | State::StepIn | State::Break => (),
        };

        self.print_source(env, expr);
        self.prompt(env, context, expr, false);
    }

    pub fn finish_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
        res: &Result<Value, Error>,
    ) {
        let state = self.stack.pop().unwrap();
        assert_eq!(state.id, expr.id);

        // Remove any active breakpoints for this expression
        for breakpoint in state.active_breakpoints {
            self.active_breakpoints.remove(&breakpoint);
        }

        // Only print the returned value if this resolves a finish command
        match self.state {
            State::Finish(finish_id) if finish_id == state.id => (),
            _ => return,
        }

        match res {
            Ok(value) => println!(
                "{}: {}",
                green!("Return value"),
                black!(format!("{}", value))
            ),
            Err(e) => println!("{}: {}", red!("error"), e),
        }

        self.print_source(env, expr);
        self.prompt(env, context, expr, true);
    }

    fn handle_command(
        &mut self,
        command: &str,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
        finish: bool,
    ) -> bool {
        let (cmd, args) = match command.split_once(" ") {
            None => (command, ""),
            Some((cmd, args)) => (cmd, args),
        };
        match cmd {
            "h" | "help" => {
                print_help(args);
                false
            }
            "r" | "run" | "c" | "continue" => {
                self.state = State::Continue;
                true
            }
            "n" | "next" => {
                if finish {
                    // When an expression is finished eval-ing, then next acts the same as step
                    self.state = State::StepIn;
                    true
                } else {
                    self.state = State::StepOver(expr.id);
                    true
                }
            }
            "s" | "step" => {
                self.state = State::StepIn;
                true
            }
            "f" | "finish" => {
                // A finish command indicates that we should continue until the parent expression
                // completes. If we entered here from a finish hook, then the current
                // expression has already popped from the stack, so the last expression id
                // on the stack is the parent. If we entered here from an entry hook, the
                // parent expression is the penultimate id on the stack.
                if finish {
                    if self.stack.len() >= 1 {
                        self.state = State::Finish(self.stack.last().unwrap().id);
                    } else {
                        self.state = State::Continue;
                    }
                } else {
                    if self.stack.len() >= 2 {
                        self.state = State::Finish(self.stack[self.stack.len() - 2].id);
                    } else {
                        self.state = State::Continue;
                    }
                }
                true
            }
            "b" | "break" => {
                self.break_command(args, env);
                false
            }
            "w" | "watch" => {
                self.watch_command(args, env, AccessType::Write);
                false
            }
            "rw" | "rwatch" => {
                self.watch_command(args, env, AccessType::Read);
                false
            }
            "aw" | "awatch" => {
                self.watch_command(args, env, AccessType::ReadWrite);
                false
            }
            "p" | "print" => {
                let contract_id = QualifiedContractIdentifier::transient();
                let (ast, mut diagnostics, success) = build_ast(&contract_id, args, &mut ());
                if ast.expressions.len() != 1 {
                    println!("{}: expected a single expression", red!("error"));
                    return false;
                }
                if !success {
                    for diagnostic in diagnostics.drain(..).filter(|d| d.level == Level::Error) {
                        println!("{}: {}", red!("error"), diagnostic.message);
                    }
                    return false;
                }

                match eval(&ast.expressions[0], env, &context) {
                    Ok(value) => println!("{}", value),
                    Err(e) => println!("{}: {}", red!("error"), e),
                }
                false
            }
            "q" | "quit" => {
                self.state = State::Quit;
                true
            }
            _ => {
                println!("Unknown command");
                print_help("");
                false
            }
        }
    }

    fn break_command(&mut self, args: &str, env: &mut Environment) {
        if args.is_empty() {
            println!("{}: invalid break command", red!("error"));
            print_help_breakpoint();
            return;
        }

        let arg_list: Vec<&str> = args.split_ascii_whitespace().collect();
        match arg_list[0] {
            "l" | "list" => {
                if self.breakpoints.is_empty() {
                    println!("No breakpoints set.")
                } else {
                    for (_, breakpoint) in &self.breakpoints {
                        println!("{}", breakpoint);
                    }
                }
            }
            "del" | "delete" => {
                let id = match arg_list[1].parse::<usize>() {
                    Ok(id) => id,
                    Err(_) => {
                        println!("{}: unable to parse breakpoint identifier", red!("error"));
                        return;
                    }
                };
                if self.delete_breakpoint(id) {
                    println!("breakpoint deleted");
                } else {
                    println!(
                        "{}: '{}' is not a currently valid breakpoint id",
                        red!("error"),
                        id
                    );
                }
            }
            _ => {
                if arg_list.len() != 1 {
                    println!("{}: invalid break command", red!("error"));
                    print_help_breakpoint();
                    return;
                }

                if args.contains(':') {
                    // Handle source breakpoints
                    // - contract:line:column
                    // - contract:line
                    // - :line
                    let parts: Vec<&str> = args.split(':').collect();
                    if parts.len() < 2 || parts.len() > 3 {
                        println!("{}: invalid breakpoint format", red!("error"));
                        print_help_breakpoint();
                        return;
                    }

                    let contract_id = if parts[0].is_empty() {
                        env.contract_context.contract_identifier.clone()
                    } else {
                        let contract_parts: Vec<&str> = parts[0].split('.').collect();
                        if contract_parts.len() != 2 {
                            println!("{}: invalid breakpoint format", red!("error"));
                            print_help_breakpoint();
                            return;
                        }
                        if contract_parts[0].is_empty() {
                            QualifiedContractIdentifier::new(
                                env.contract_context.contract_identifier.issuer.clone(),
                                ContractName::try_from(contract_parts[1]).unwrap(),
                            )
                        } else {
                            match QualifiedContractIdentifier::parse(parts[0]) {
                                Ok(contract_identifier) => contract_identifier,
                                Err(e) => {
                                    println!(
                                        "{}: unable to parse breakpoint contract identifier: {}",
                                        red!("error"),
                                        e
                                    );
                                    print_help_breakpoint();
                                    return;
                                }
                            }
                        }
                    };

                    let line = match parts[1].parse::<u32>() {
                        Ok(line) => line,
                        Err(e) => {
                            println!("{}: invalid breakpoint format", red!("error"),);
                            print_help_breakpoint();
                            return;
                        }
                    };

                    let column = if parts.len() == 3 {
                        match parts[2].parse::<u32>() {
                            Ok(column) => column,
                            Err(e) => {
                                println!("{}: invalid breakpoint format", red!("error"),);
                                print_help_breakpoint();
                                return;
                            }
                        }
                    } else {
                        0
                    };

                    self.add_breakpoint(Breakpoint {
                        id: 0,
                        verified: true,
                        data: BreakpointData::Source(SourceBreakpoint {
                            line,
                            column: if column == 0 { None } else { Some(column) },
                            log_message: None,
                        }),
                        source: Source { name: contract_id },
                        span: Some(Span {
                            start_line: line,
                            start_column: column,
                            end_line: line,
                            end_column: column,
                        }),
                    });
                } else {
                    // Handle function breakpoints
                    // - principal.contract.function
                    // - .contract.function
                    // - function
                    let parts: Vec<&str> = args.split('.').collect();
                    let (contract_id, function_name) = match parts.len() {
                        1 => (env.contract_context.contract_identifier.clone(), parts[0]),
                        3 => {
                            let contract_id = if parts[0].is_empty() {
                                QualifiedContractIdentifier::new(
                                    env.contract_context.contract_identifier.issuer.clone(),
                                    ContractName::try_from(parts[1]).unwrap(),
                                )
                            } else {
                                match QualifiedContractIdentifier::parse(
                                    args.rsplit_once('.').unwrap().0,
                                ) {
                                    Ok(contract_identifier) => contract_identifier,
                                    Err(e) => {
                                        println!(
                                            "{}: unable to parse breakpoint contract identifier: {}",
                                            red!("error"),
                                            e
                                        );
                                        print_help_breakpoint();
                                        return;
                                    }
                                }
                            };
                            (contract_id, parts[2])
                        }
                        _ => {
                            println!("{}: invalid breakpoint format", red!("error"),);
                            print_help_breakpoint();
                            return;
                        }
                    };

                    let contract = match env.global_context.database.get_contract(&contract_id) {
                        Ok(contract) => contract,
                        Err(e) => {
                            println!("{}: {}", red!("error"), e);
                            return;
                        }
                    };
                    let function = match contract.contract_context.lookup_function(function_name) {
                        None => {
                            println!("{}: no such function", red!("error"));
                            return;
                        }
                        Some(function) => function,
                    };

                    self.add_breakpoint(Breakpoint {
                        id: 0,
                        verified: true,
                        data: BreakpointData::Function(FunctionBreakpoint {
                            name: function_name.to_string(),
                        }),
                        source: Source { name: contract_id },
                        span: Some(function.body.span.clone()),
                    });
                }
            }
        }
    }

    fn watch_command(&mut self, args: &str, env: &mut Environment, access_type: AccessType) {
        if args.is_empty() {
            println!("{}: invalid watch command", red!("error"));
            print_help_watchpoint();
            return;
        }

        let arg_list: Vec<&str> = args.split_ascii_whitespace().collect();
        match arg_list[0] {
            "l" | "list" => {
                if self.watchpoints.is_empty() {
                    println!("No watchpoints set.")
                } else {
                    for (_, watchpoint) in &self.watchpoints {
                        println!("{}", watchpoint);
                    }
                }
            }
            "del" | "delete" => {
                let id = match arg_list[1].parse::<usize>() {
                    Ok(id) => id,
                    Err(_) => {
                        println!("{}: unable to parse watchpoint identifier", red!("error"));
                        return;
                    }
                };
                if self.delete_watchpoint(id) {
                    println!("watchpoint deleted");
                } else {
                    println!(
                        "{}: '{}' is not a currently valid watchpoint id",
                        red!("error"),
                        id
                    );
                }
            }
            _ => {
                if arg_list.len() != 1 {
                    println!("{}: invalid watch command", red!("error"));
                    print_help_watchpoint();
                    return;
                }

                // Syntax could be:
                // - principal.contract.name
                // - .contract.name
                // - name
                let parts: Vec<&str> = args.split('.').collect();
                let (contract_id, name) = match parts.len() {
                    1 => (env.contract_context.contract_identifier.clone(), parts[0]),
                    3 => {
                        let contract_id = if parts[0].is_empty() {
                            QualifiedContractIdentifier::new(
                                env.contract_context.contract_identifier.issuer.clone(),
                                ContractName::try_from(parts[1]).unwrap(),
                            )
                        } else {
                            match QualifiedContractIdentifier::parse(
                                args.rsplit_once('.').unwrap().0,
                            ) {
                                Ok(contract_identifier) => contract_identifier,
                                Err(e) => {
                                    println!(
                                        "{}: unable to parse watchpoint contract identifier: {}",
                                        red!("error"),
                                        e
                                    );
                                    print_help_watchpoint();
                                    return;
                                }
                            }
                        };
                        (contract_id, parts[2])
                    }
                    _ => {
                        println!("{}: invalid watchpoint format", red!("error"),);
                        print_help_watchpoint();
                        return;
                    }
                };

                let contract = match env.global_context.database.get_contract(&contract_id) {
                    Ok(contract) => contract,
                    Err(e) => {
                        println!("{}: {}", red!("error"), e);
                        return;
                    }
                };

                if contract.contract_context.meta_data_var.get(name).is_none()
                    && contract.contract_context.meta_data_map.get(name).is_none()
                {
                    println!(
                        "{}: no such variable: {}.{}",
                        red!("error"),
                        contract_id,
                        name
                    );
                    return;
                }

                self.add_watchpoint(Breakpoint {
                    id: 0,
                    verified: true,
                    data: BreakpointData::Data(DataBreakpoint {
                        name: name.to_string(),
                        access_type,
                    }),
                    source: Source { name: contract_id },
                    span: None,
                });
            }
        }
    }
}

fn print_help(args: &str) {
    match args {
        "b" | "breakpoint" => print_help_breakpoint(),
        _ => print_help_main(),
    }
}

fn print_help_main() {
    println!(
        r#"Debugger commands:
  aw | awatch      -- Read/write watchpoint, see `help watch' for details)
  b | breakpoint   -- Commands for operating on breakpoints (see 'help b' for details)
  c | continue     -- Continue execution until next breakpoint or completion
  f | finish       -- Continue execution until returning from the current expression
  n | next         -- Single step, stepping over sub-expressions
  p | print <expr> -- Evaluate an expression and print the result
  q | quit         -- Quit the debugger
  r | run          -- Begin execution
  rw | rwatch      -- Read watchpoint, see `help watch' for details)
  s | step         -- Single step, stepping into sub-expressions
  w | watch        -- Commands for operating on watchpoints (see 'help w' for details)
"#
    );
}

fn print_help_breakpoint() {
    println!(
        r#"Set a breakpoint using 'b' or 'break' and one of these formats
  b <principal?>.<contract>:<linenum>:<colnum>
    SP000000000000000000002Q6VF78.bns:604:9
        Break at line 604, column 9 of the bns contract deployed by 
          SP000000000000000000002Q6VF78

  b <principal?>.<contract>:<linenum>
    .my-contract:193
        Break at line 193 of the my-contract contract deployed by the current
          tx-sender

  b :<linenum>:<colnum>
    :12:4
        Break at line 12, column 4 of the current contract

  b :<linenum>
    :12
        Break at line 12 of the current contract

  b <principal>.<contract>.<function>
    SP000000000000000000002Q6VF78.bns.name-preorder
        Break at the function name-preorder from the bns contract deployed by
          SP000000000000000000002Q6VF78

  b .<contract>.<function>
    .foo.do-something
        Break at the function 'do-something from the 'foo' contract deployed by
          the current principal

  b <function>
    take-action
        Break at the function 'take-action' current contract

List current breakpoints
  b list
  b l

Delete a breakpoint using its identifier
  b delete <breakpoint-id>
  b del <breakpoint-id>
"#
    );
}

fn print_help_watchpoint() {
    println!(
        r#"Set a watchpoint using 'w' or 'watch' and one of these formats
  w <principal>.<contract>.<name>
    SP000000000000000000002Q6VF78.bns.owner-name
        Break on writes to the map 'owner-name' from the 'bns' contract
          deployed by SP000000000000000000002Q6VF78
  w .<contract>.<name>
    .foo.bar
        Break on writes to the variable 'bar' from the 'foo' contract
          deployed by the current principal
  w <name>
    something
        Watch the variable 'something' from the current contract

Default watchpoints break when the variable or map is written. Using the same
formats, the command 'rwatch' sets a read watchpoint to break when the variable
or map is read, and 'awatch' sets a read/write watchpoint to break on read or
write.

List current watchpoints
  w list
  w l

Delete a watchpoint using its identifier
  w delete <watchpoint-id>
  w del <watchpoint-id>
"#
    );
}
