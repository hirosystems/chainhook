use std::collections::HashMap;

use crate::clarity::contexts::{CallStack, ContractContext, Environment, LocalContext};
use crate::clarity::database::ClarityDatabase;
use crate::clarity::errors::Error;
use crate::clarity::representations::Span;
use crate::clarity::representations::SymbolicExpression;
use crate::clarity::types::QualifiedContractIdentifier;
use crate::clarity::types::Value;
use crate::clarity::SymbolicExpressionType;
use rustyline::error::ReadlineError;
use rustyline::Editor;

const HISTORY_FILE: Option<&'static str> = option_env!("CLARITY_DEBUG_HISTORY_FILE");

pub struct Source {
    name: Option<String>,
    path: Option<String>,
}

pub struct Breakpoint {
    id: usize,
    verified: bool,
    data: BreakpointData,
    source: Source,
    span: Option<Span>,
}
pub enum BreakpointData {
    Source(SourceBreakpoint),
    Function(FunctionBreakpoint),
}

pub struct SourceBreakpoint {
    line: u32,
    column: Option<u32>,
    log_message: String,
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

#[derive(PartialEq, Debug)]
enum State {
    Start,
    Continue,
    StepOver(u64),
    StepIn,
    Finish(u64),
    Quit,
}

pub struct DebugState {
    editor: Editor<()>,
    breakpoints: Vec<Breakpoint>,
    state: State,
    stack: Vec<u64>,
}

impl DebugState {
    pub fn new() -> DebugState {
        let mut editor = Editor::<()>::new();
        editor
            .load_history(HISTORY_FILE.unwrap_or(".debug_history"))
            .ok();

        DebugState {
            editor,
            breakpoints: Vec::new(),
            state: State::Start,
            stack: Vec::new(),
        }
    }

    fn prompt(&mut self, context: &LocalContext, expr: &SymbolicExpression, finish: bool) {
        let prompt = "(debug) ";
        loop {
            let readline = self.editor.readline(prompt);
            let resume = match readline {
                Ok(mut command) => {
                    if command.is_empty() {
                        match self.editor.history().last() {
                            Some(prev) => command = prev.clone(),
                            None => (),
                        }
                    }
                    self.editor.add_history_entry(&command);
                    self.handle_command(&command, context, expr, finish)
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
    }

    // Print the source of the current expr (if it has a valid span).
    fn print_source(
        &mut self,
        db: &mut ClarityDatabase,
        contract: &QualifiedContractIdentifier,
        expr: &SymbolicExpression,
    ) {
        if expr.span.start_line != 0 {
            match db.get_contract_src(contract) {
                Some(contract_source) => {
                    println!(
                        "{}:{}:{}",
                        blue!(format!("{}", contract)),
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
                        contract, expr.span.start_line, expr.span.start_column
                    );
                }
            }
        } else {
            println!("{}", yellow!("source information unknown"));
        }
    }

    pub fn begin_eval(
        &mut self,
        db: &mut ClarityDatabase,
        contract: &QualifiedContractIdentifier,
        context: &LocalContext,
        expr: &SymbolicExpression,
    ) {
        self.stack.push(expr.id);

        // If user quit debug session, we can't stop executing, but don't do anything else.
        if self.state == State::Quit {
            return;
        }

        // Check if we have hit a breakpoint

        // Always skip over non-list expressions (values).
        match expr.expr {
            SymbolicExpressionType::List(_) => (),
            _ => return,
        };

        match self.state {
            State::Continue | State::Quit | State::Finish(_) => return,
            State::StepOver(step_over_id) => {
                if self.stack.contains(&step_over_id) {
                    // We're still inside the expression which should be stepped over,
                    // so return to execution.
                    return;
                }
            }
            State::Start | State::StepIn => (),
        };

        self.print_source(db, contract, expr);
        self.prompt(context, expr, false);
    }

    pub fn finish_eval(
        &mut self,
        db: &mut ClarityDatabase,
        contract: &QualifiedContractIdentifier,
        context: &LocalContext,
        expr: &SymbolicExpression,
        res: &Result<Value, Error>,
    ) {
        let id = self.stack.pop().unwrap();
        assert_eq!(id, expr.id);

        // Only print the returned value if this resolves a finish command
        match self.state {
            State::Finish(finish_id) if finish_id == id => (),
            _ => return,
        }

        match res {
            Ok(value) => println!("{}: {}", green!("Return value"), black!(format!("{}", value))),
            Err(e) => println!("{}: {}", red!("error"), e),
        }

        self.print_source(db, contract, expr);
        self.prompt(context, expr, true);
    }

    fn handle_command(
        &mut self,
        command: &str,
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
                        self.state = State::Finish(*self.stack.last().unwrap());
                    } else {
                        self.state = State::Continue;
                    }
                } else {
                    if self.stack.len() >= 2 {
                        self.state = State::Finish(self.stack[self.stack.len() - 2]);
                    } else {
                        self.state = State::Continue;
                    }
                }
                true
            }
            "p" | "print" => {
                match context.lookup_variable(args) {
                    Some(value) => println!("{}", value),
                    None => println!("{}: unknown variable", red!("error")),
                };
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
    12:4
        Break at line 12, column 4 of the current contract

  b <linenum>
    12
        Break at line 12 of the current contract

  b <principal?>.<contract>.<function>
    SP000000000000000000002Q6VF78.bns.name-preorder
        Break at the function name-preorder from the bns contract deployed by
          SP000000000000000000002Q6VF78
"#
    );
}
