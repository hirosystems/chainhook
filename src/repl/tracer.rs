use crate::clarity::errors::Error;
use crate::clarity::eval;
use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::types::PrincipalData;
use crate::clarity::{
    contexts::{Environment, LocalContext},
    types::Value,
    EvalHook, SymbolicExpression, SymbolicExpressionType,
};
use crate::repl::tracer::SymbolicExpressionType::List;

pub struct Tracer {
    snippet: String,
    stack: Vec<u64>,
    arg_count: Vec<usize>,
    arg_stack: Vec<Vec<u64>>,
    pending_call: Vec<String>,
    pending_args: Vec<String>,
}

impl Tracer {
    pub fn new(snippet: String) -> Tracer {
        println!("{}  {}", snippet, black!("<console>"));
        Tracer {
            snippet,
            stack: vec![0],
            arg_count: Vec::new(),
            arg_stack: Vec::new(),
            pending_call: Vec::new(),
            pending_args: Vec::new(),
        }
    }

    fn collect_args(&mut self, num: usize) {
        self.arg_count.push(num);
        self.arg_stack.push(Vec::new());
    }
}

impl EvalHook for Tracer {
    fn will_begin_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
    ) {
        if let Some(arg_count) = self.arg_count.last() {
            if let Some(arg_stack) = self.arg_stack.last_mut() {
                if arg_stack.is_empty() {
                    arg_stack.push(expr.id);
                }
            }
        }

        let list = match &expr.expr {
            List(list) => list,
            _ => return,
        };
        if let Some((function_name, args)) = list.split_first() {
            if let Some(function_name) = function_name.match_atom() {
                if DefineFunctions::lookup_by_name(function_name).is_some() {
                    return;
                } else if let Some(native_function) = NativeFunctions::lookup_by_name(function_name)
                {
                    match native_function {
                        NativeFunctions::ContractCall => {
                            let call = format!(
                                "{}├── {}  {}",
                                "│   ".repeat(self.stack.len() - self.pending_call.len() - 1),
                                expr,
                                black!(format!(
                                    "{}:{}:{}",
                                    env.contract_context.contract_identifier.name,
                                    expr.span.start_line,
                                    expr.span.start_column,
                                )),
                            );

                            let mut lines = Vec::new();
                            if args[0].match_atom().is_some() {
                                let callee = if let Ok(value) = eval(&args[0], env, context) {
                                    value.to_string()
                                } else {
                                    "?".to_string()
                                };
                                lines.push(format!(
                                    "{}│ {}",
                                    "│   ".repeat(self.stack.len() - self.pending_call.len()),
                                    purple!(format!("↳ callee: {}", callee)),
                                ));
                            }

                            if args.len() > 0 {
                                lines.push(format!(
                                    "{}│ {}",
                                    "│   ".repeat(self.stack.len() - self.pending_call.len()),
                                    purple!("↳ args:"),
                                ));
                                self.pending_call.push(call);
                                self.pending_args.push(lines.join("\n"));
                                self.collect_args(args.len() - 2);
                            } else {
                                println!(
                                    "{}{}",
                                    "│   ".repeat(self.stack.len() - self.pending_call.len()),
                                    call
                                );
                            }
                        }
                        _ => return,
                    }
                } else {
                    // Call user-defined function
                    let call = format!(
                        "{}├── {}  {}",
                        "│   ".repeat(self.stack.len() - self.pending_call.len() - 1),
                        expr,
                        black!(format!(
                            "{}:{}:{}",
                            env.contract_context.contract_identifier.name,
                            expr.span.start_line,
                            expr.span.start_column,
                        )),
                    );
                    self.pending_args.push(format!(
                        "{}│ {}",
                        "│   ".repeat(self.stack.len() - self.pending_call.len()),
                        purple!("↳ args:"),
                    ));
                    if args.len() > 0 {
                        self.pending_call.push(call);
                        self.collect_args(args.len());
                    } else {
                        println!(
                            "{}{}",
                            "│   ".repeat(self.stack.len() - self.pending_call.len()),
                            call
                        );
                    }
                }
            }
        }
        self.stack.push(expr.id);
    }

    fn did_finish_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
        res: &Result<Value, Error>,
    ) {
        if let Some(last) = self.stack.last() {
            if *last == expr.id {
                if let Ok(value) = res {
                    println!(
                        "{}└── {}",
                        "│   ".repeat(self.stack.len() - self.pending_call.len() - 1),
                        blue!(value.to_string())
                    );
                }
                self.stack.pop();
            }
        }

        // Collect argument values
        if let (Some(arg_count), Some(arg_stack)) =
            (self.arg_count.last(), self.arg_stack.last_mut())
        {
            if let Some(arg) = arg_stack.last() {
                if *arg == expr.id {
                    let arg_count = self.arg_count.pop().unwrap() - 1;
                    if let Ok(value) = res {
                        self.pending_args
                            .last_mut()
                            .unwrap()
                            .push_str(format!(" {}", value).as_str());
                    }
                    // If this was the last argument, print it out and pop the stack
                    if arg_count == 0 {
                        let call = self.pending_call.pop().unwrap();
                        println!("{}", call);
                        println!("{}", self.pending_args.pop().unwrap().to_string());
                        self.arg_stack.pop();
                    } else {
                        // Pop this arg and push the decremented arg count back on the stack
                        self.arg_count.push(arg_count);
                        arg_stack.pop();
                    }
                }
            }
        }
    }

    fn did_complete(&mut self, result: core::result::Result<&mut super::ExecutionResult, String>) {
        match result {
            Ok(result) => {
                if let Some(value) = &result.result {
                    println!("└── {}", blue!(format!("{}", value)));
                }
            }
            Err(e) => println!("{}", e),
        }
    }
}
