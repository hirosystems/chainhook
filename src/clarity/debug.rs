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
}

impl Display for BreakpointData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakpointData::Source(source) => write!(f, "{}", source),
            BreakpointData::Function(function) => write!(f, "{}", function),
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

pub enum AccessType {
    Read,
    Write,
    ReadWrite,
}
pub struct DataBreakpoint {
    name: String,
    access_type: AccessType,
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
    break_functions: HashMap<(QualifiedContractIdentifier, String), HashSet<usize>>,
    break_locations: HashMap<QualifiedContractIdentifier, HashSet<usize>>,
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
            break_functions: HashMap::new(),
            break_locations: HashMap::new(),
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

        // Check if we have hit a breakpoint
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
  b | breakpoint   -- Commands for operating on breakpoints (see 'help b' for details)
  c | continue     -- Continue execution until next breakpoint or completion
  f | finish       -- Continue execution until returning from the current expression
  n | next         -- Single step, stepping over sub-expressions
  p | print <name> -- Print the value of a variable
  q | quit         -- Quit the debugger
  r | run          -- Begin execution
  s | step         -- Single step, stepping into sub-expressions
"#
    );
}

fn print_help_breakpoint() {
    println!(
        r#"Set a breakpoint using one of the following formats:
  b <principal?>.<contract>:<linenum>:<colnum>
    SP000000000000000000002Q6VF78.bns:604:9
        Break at line 604, column 9 of the bns contract deployed by 
          SP000000000000000000002Q6VF78

  b <principal?>.<contract>:<linenum>
    .my-contract:193
        Break at line 193 of the my-contract contract deployed by the current
          tx-sender

  b <linenum>:<colnum>
    :12:4
        Break at line 12, column 4 of the current contract

  b <linenum>
    :12
        Break at line 12 of the current contract

  b <principal?>.<contract>.<function>
    SP000000000000000000002Q6VF78.bns.name-preorder
        Break at the function name-preorder from the bns contract deployed by
          SP000000000000000000002Q6VF78

List current breakpoints
  b l
  b list

Delete a breakpoint using its identifier
  b del <breakpoint-id>
  b delete <breakpoint-id>
"#
    );
}
