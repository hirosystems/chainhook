use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::hash::Hash;
use std::io::Write;
use std::path::PathBuf;

use super::State;
use crate::clarity::callables::FunctionIdentifier;
use crate::clarity::errors::Error;
use crate::clarity::representations::Span;
use crate::clarity::types::Value;
use crate::clarity::SymbolicExpressionType::List;
use crate::clarity::{
    contexts::{Environment, LocalContext},
    types::QualifiedContractIdentifier,
    EvalHook, SymbolicExpression,
};
use dap_types::events::*;
use dap_types::requests::*;
use dap_types::responses::*;
use dap_types::types::*;
use dap_types::*;
use futures::{SinkExt, StreamExt};
use tokio;
use tokio::io::{Stdin, Stdout};
use tokio_util::codec::{FramedRead, FramedWrite};

use self::codec::{DebugAdapterCodec, ParseError};

use super::DebugState;

mod codec;

/*
 * DAP Session:
 *      VSCode                    DAPDebugger
 *        |                            |
 *        |--- initialize ------------>|
 *        |<-- initialize response ----|
 *        |--- launch ---------------->|
 *        |<-- launch response --------|
 *        |<-- initialized event ------|
 *        |<-- stopped event ----------|
 *        |--- set breakpoints ------->|
 *        |<-- set bps response -------|
 *        |--- threads --------------->|
 *        |<-- threads response -------|
 *        |--- set exception bps ----->|
 *        |<-- set exc bps response ---|
 *        |--- threads --------------->|
 *        |<-- threads response -------|
 */

struct Current {
    source: Source,
    span: Span,
    expr_id: u64,
    stack: Vec<FunctionIdentifier>,
    scopes: Vec<Scope>,
}

struct Frame {
    stack_frame: StackFrame,
    scopes: Vec<Scope>,
}

pub struct DAPDebugger {
    // map source path to contract_id
    pub path_to_contract_id: HashMap<String, QualifiedContractIdentifier>,
    pub contract_id_to_path: HashMap<QualifiedContractIdentifier, String>,
    log_file: File, // DELETE ME: For testing only
    reader: FramedRead<Stdin, DebugAdapterCodec<ProtocolMessage>>,
    writer: FramedWrite<Stdout, DebugAdapterCodec<ProtocolMessage>>,
    state: Option<DebugState>,
    send_seq: i64,
    launched: Option<(String, String)>,
    launch_seq: i64,
    current: Option<Current>,
    init_complete: bool,
    stack_frames: HashMap<FunctionIdentifier, Frame>,
}

impl DAPDebugger {
    pub fn new() -> Self {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let reader = FramedRead::new(stdin, DebugAdapterCodec::<ProtocolMessage>::default());
        let writer = FramedWrite::new(stdout, DebugAdapterCodec::<ProtocolMessage>::default());
        let mut log_file = match File::create("/Users/brice/work/debugger-demo/dap-log.txt") {
            // DELETE ME
            Ok(file) => file,
            Err(e) => {
                let mut file = OpenOptions::new()
                    .write(true)
                    .append(true)
                    .open("/Users/brice/work/debugger-demo/debugger.txt")
                    .unwrap();
                writeln!(file, "DAP_LOG FAILED: {}", e);
                file
            }
        };
        writeln!(log_file, "LOG FILE CREATED");
        Self {
            path_to_contract_id: HashMap::new(),
            contract_id_to_path: HashMap::new(),
            log_file,
            reader,
            writer,
            state: None,
            send_seq: 0,
            launched: None,
            launch_seq: 0,
            current: None,
            init_complete: false,
            stack_frames: HashMap::new(),
        }
    }

    fn get_state(&mut self) -> &mut DebugState {
        self.state.as_mut().unwrap()
    }

    // Process all messages before launching the REPL
    pub async fn init(&mut self) -> Result<(String, String), ParseError> {
        writeln!(self.log_file, "STARTING");

        while self.launched.is_none() {
            match self.wait_for_command().await {
                Ok(_) => (),
                Err(e) => return Err(e),
            }
        }
        writeln!(
            self.log_file,
            "inited: {}, {}",
            self.launched.as_ref().unwrap().0,
            self.launched.as_ref().unwrap().1
        );
        Ok(self.launched.take().unwrap())
    }

    // Successful result boolean indicates if execution should continue based on the message received
    async fn wait_for_command(&mut self) -> Result<bool, ParseError> {
        writeln!(self.log_file, "WAITING FOR MESSAGE...");
        if let Some(msg) = self.reader.next().await {
            match msg {
                Ok(msg) => {
                    writeln!(self.log_file, "message: {:?}", msg);

                    use dap_types::MessageKind::*;
                    Ok(match msg.message {
                        Request(command) => self.handle_request(msg.seq, command).await,
                        Response(response) => {
                            self.handle_response(msg.seq, response).await;
                            false
                        }
                        Event(event) => {
                            self.handle_event(msg.seq, event).await;
                            false
                        }
                    })
                }
                Err(e) => {
                    writeln!(self.log_file, "error: {}", e);
                    Err(e)
                }
            }
        } else {
            writeln!(self.log_file, "NONE");
            Ok(true)
        }
    }

    async fn send_response(&mut self, response: Response) {
        let response_json = serde_json::to_string(&response).unwrap();
        writeln!(self.log_file, "::::response: {}", response_json);

        let message = ProtocolMessage {
            seq: self.send_seq,
            message: MessageKind::Response(response),
        };

        match self.writer.send(message).await {
            Ok(_) => (),
            Err(e) => {
                writeln!(self.log_file, "ERROR: sending response: {}", e);
            }
        };

        self.send_seq += 1;
    }

    async fn send_event(&mut self, body: EventBody) {
        let event_json = serde_json::to_string(&body).unwrap();
        writeln!(self.log_file, "::::event: {}", event_json);

        let message = ProtocolMessage {
            seq: self.send_seq,
            message: MessageKind::Event(Event { body: Some(body) }),
        };

        match self.writer.send(message).await {
            Ok(_) => (),
            Err(e) => {
                writeln!(self.log_file, "ERROR: sending response: {}", e);
            }
        };

        self.send_seq += 1;
    }

    pub async fn handle_request(&mut self, seq: i64, command: RequestCommand) -> bool {
        use dap_types::requests::RequestCommand::*;
        let proceed = match command {
            Initialize(arguments) => self.initialize(seq, arguments).await,
            Launch(arguments) => self.launch(seq, arguments).await,
            ConfigurationDone => self.configuration_done(seq).await,
            SetBreakpoints(arguments) => self.setBreakpoints(seq, arguments).await,
            SetExceptionBreakpoints(arguments) => {
                self.setExceptionBreakpoints(seq, arguments).await
            }
            Disconnect(arguments) => self.quit(seq, arguments).await,
            Threads => self.threads(seq).await,
            StackTrace(arguments) => self.stack_trace(seq, arguments).await,
            Scopes(arguments) => self.scopes(seq, arguments).await,
            StepIn(arguments) => self.step_in(seq, arguments).await,
            StepOut(arguments) => self.step_out(seq, arguments).await,
            Next(arguments) => self.next(seq, arguments).await,
            Continue(arguments) => self.continue_(seq, arguments).await,
            Pause(arguments) => self.pause(seq, arguments).await,
            _ => {
                self.send_response(Response {
                    request_seq: seq,
                    success: false,
                    message: Some("unsupported request".to_string()),
                    body: None,
                })
                .await;
                false
            }
        };

        proceed
    }

    pub async fn handle_event(&mut self, seq: i64, event: Event) {
        let response = Response {
            request_seq: seq,
            success: true,
            message: None,
            body: None,
        };
        self.send_response(response).await;
    }

    pub async fn handle_response(&mut self, seq: i64, response: Response) {
        let response = Response {
            request_seq: seq,
            success: true,
            message: None,
            body: None,
        };
        self.send_response(response).await;
    }

    // Request handlers

    async fn initialize(&mut self, seq: i64, arguments: InitializeRequestArguments) -> bool {
        writeln!(self.log_file, "INITIALIZE");
        let capabilities = Capabilities {
            supports_configuration_done_request: Some(true),
            supports_function_breakpoints: Some(true),
            supports_step_in_targets_request: Some(true),
            support_terminate_debuggee: Some(true),
            supports_loaded_sources_request: Some(true),
            supports_data_breakpoints: Some(true),
            supports_breakpoint_locations_request: Some(true),
            supports_conditional_breakpoints: None,
            supports_hit_conditional_breakpoints: None,
            supports_evaluate_for_hovers: None,
            exception_breakpoint_filters: None,
            supports_step_back: None,
            supports_set_variable: None,
            supports_restart_frame: None,
            supports_goto_targets_request: None,
            supports_completions_request: None,
            completion_trigger_characters: None,
            supports_modules_request: None,
            additional_module_columns: None,
            supported_checksum_algorithms: None,
            supports_restart_request: None,
            supports_exception_options: None,
            supports_value_formatting_options: None,
            supports_exception_info_request: None,
            support_suspend_debuggee: None,
            supports_delayed_stack_trace_loading: None,
            supports_log_points: None,
            supports_terminate_threads_request: None,
            supports_set_expression: None,
            supports_terminate_request: None,
            supports_read_memory_request: None,
            supports_write_memory_request: None,
            supports_disassemble_request: None,
            supports_cancel_request: None,
            supports_clipboard_context: None,
            supports_stepping_granularity: None,
            supports_instruction_breakpoints: None,
            supports_exception_filter_options: None,
            supports_single_thread_execution_requests: None,
        };

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Initialize(InitializeResponse {
                capabilities,
            })),
        })
        .await;

        false
    }

    pub fn log<S: Into<String>>(&mut self, message: S) {
        block_on(self.send_event(EventBody::Output(OutputEvent {
            category: Some(Category::Console),
            output: message.into(),
            group: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            data: None,
        })));
    }

    pub fn stdout<S: Into<String>>(&mut self, message: S) {
        block_on(self.send_event(EventBody::Output(OutputEvent {
            category: Some(Category::Stdout),
            output: message.into(),
            group: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            data: None,
        })));
    }

    pub fn stderr<S: Into<String>>(&mut self, message: S) {
        block_on(self.send_event(EventBody::Output(OutputEvent {
            category: Some(Category::Stderr),
            output: message.into(),
            group: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            data: None,
        })));
    }

    async fn launch(&mut self, seq: i64, arguments: LaunchRequestArguments) -> bool {
        writeln!(self.log_file, "LAUNCH");
        // Verify that the manifest and expression were specified
        let manifest = match arguments.manifest {
            Some(manifest) => manifest,
            None => {
                self.send_response(Response {
                    request_seq: seq,
                    success: false,
                    message: Some("manifest must be specified".to_string()),
                    body: None,
                })
                .await;
                return false;
            }
        };
        let expression = match arguments.expression {
            Some(expression) => expression,
            None => {
                self.send_response(Response {
                    request_seq: seq,
                    success: false,
                    message: Some("expression to debug must be specified".to_string()),
                    body: None,
                })
                .await;
                return false;
            }
        };

        let contract_id = QualifiedContractIdentifier::transient();
        self.state = Some(DebugState::new(&contract_id, &expression));
        self.launched = Some((manifest, expression));

        self.launch_seq = seq;

        false
    }

    async fn configuration_done(&mut self, seq: i64) -> bool {
        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::ConfigurationDone),
        })
        .await;

        // Now that configuration is done, we can respond to the launch
        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Launch),
        })
        .await;

        false
    }

    async fn setBreakpoints(&mut self, seq: i64, arguments: SetBreakpointsArguments) -> bool {
        let mut results = vec![];
        match arguments.breakpoints {
            Some(breakpoints) => {
                let source = super::Source {
                    name: self
                        .path_to_contract_id
                        .get(&arguments.source.path.clone().unwrap())
                        .unwrap()
                        .clone(),
                };
                for breakpoint in breakpoints {
                    let column = match breakpoint.column {
                        Some(column) => column,
                        None => 0,
                    };
                    let source_breakpoint = super::Breakpoint {
                        id: 0,
                        verified: true,
                        data: super::BreakpointData::Source(super::SourceBreakpoint {
                            line: breakpoint.line,
                            column: breakpoint.column,
                        }),
                        source: source.clone(),
                        span: Some(Span {
                            start_line: breakpoint.line,
                            start_column: column,
                            end_line: breakpoint.line,
                            end_column: column,
                        }),
                    };
                    let id = self.get_state().add_breakpoint(source_breakpoint);
                    results.push(Breakpoint {
                        id: Some(id),
                        verified: true,
                        message: breakpoint.log_message,
                        source: Some(arguments.source.clone()),
                        line: Some(breakpoint.line),
                        column: breakpoint.column,
                        end_line: Some(breakpoint.line),
                        end_column: breakpoint.column,
                        instruction_reference: None,
                        offset: None,
                    });
                }
            }
            None => (),
        };

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::SetBreakpoints(SetBreakpointsResponse {
                breakpoints: results,
            })),
        })
        .await;

        false
    }

    async fn setExceptionBreakpoints(
        &mut self,
        seq: i64,
        arguments: SetExceptionBreakpointsArguments,
    ) -> bool {
        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::SetExceptionBreakpoints(
                SetExceptionBreakpointsResponse { breakpoints: None },
            )),
        })
        .await;

        false
    }

    async fn threads(&mut self, seq: i64) -> bool {
        // There is only ever 1 thread
        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Threads(ThreadsResponse {
                threads: vec![Thread {
                    id: 0,
                    name: "main".to_string(),
                }],
            })),
        })
        .await;

        // VSCode doesn't seem to want to send us a ConfigurationDone request,
        // so assume that this is the end of configuration instead. This is an
        // ugly hack and should be changed!
        if !self.init_complete {
            self.send_response(Response {
                request_seq: self.launch_seq,
                success: true,
                message: None,
                body: Some(ResponseBody::Launch),
            })
            .await;

            let mut stopped = StoppedEvent {
                reason: StoppedReason::Entry,
                description: None,
                thread_id: Some(0),
                preserve_focus_hint: None,
                text: Some("Stopped at start!!!".to_string()),
                all_threads_stopped: None,
                hit_breakpoint_ids: None,
            };

            self.send_event(EventBody::Stopped(stopped)).await;
            self.init_complete = true;
        }

        false
    }

    async fn stack_trace(&mut self, seq: i64, arguments: StackTraceArguments) -> bool {
        let current = self.current.as_ref().unwrap();
        let frames: Vec<_> = current
            .stack
            .iter()
            .rev()
            .filter(|function| !function.identifier.starts_with("_native_:"))
            .map(|function| self.stack_frames[function].stack_frame.clone())
            .collect();

        let len = current.stack.len() as i32;
        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::StackTrace(StackTraceResponse {
                stack_frames: frames,
                total_frames: Some(len),
            })),
        })
        .await;
        false
    }

    async fn scopes(&mut self, seq: i64, arguments: ScopesArguments) -> bool {
        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Scopes(ScopesResponse {
                scopes: self.current.as_ref().unwrap().scopes.clone(),
            })),
        })
        .await;
        false
    }

    async fn step_in(&mut self, seq: i64, arguments: StepInArguments) -> bool {
        self.get_state().step_in();

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::StepIn),
        })
        .await;
        true
    }

    async fn step_out(&mut self, seq: i64, arguments: StepOutArguments) -> bool {
        self.get_state().finish();

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::StepOut),
        })
        .await;
        true
    }

    async fn next(&mut self, seq: i64, arguments: NextArguments) -> bool {
        let expr_id = self.current.as_ref().unwrap().expr_id;
        self.get_state().step_over(expr_id);

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Next),
        })
        .await;
        true
    }

    async fn continue_(&mut self, seq: i64, arguments: ContinueArguments) -> bool {
        self.get_state().continue_execution();

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Continue(ContinueResponse {
                all_threads_continued: None,
            })),
        })
        .await;
        true
    }

    async fn pause(&mut self, seq: i64, arguments: PauseArguments) -> bool {
        self.get_state().pause();

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Pause),
        })
        .await;
        true
    }

    async fn quit(&mut self, seq: i64, arguments: DisconnectArguments) -> bool {
        // match arguments.restart {
        //     Some(restart) => restart,
        //     None => false,
        // }
        self.get_state().quit();

        self.send_response(Response {
            request_seq: seq,
            success: true,
            message: None,
            body: Some(ResponseBody::Disconnect),
        })
        .await;
        true
    }
}

impl EvalHook for DAPDebugger {
    fn begin_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
    ) {
        writeln!(self.log_file, "in begin_eval: {:?}", expr);
        if !self.get_state().begin_eval(env, context, expr) {
            if self.get_state().state == State::Start {
                // Sending this initialized event triggers the configuration
                // (e.g. setting breakpoints), after which the ConfigurationDone
                // request should be sent, but it's not, so there is an ugly
                // hack in threads to handle that.
                block_on(self.send_event(EventBody::Initialized));
            } else {
                let mut stopped = StoppedEvent {
                    reason: StoppedReason::Entry,
                    description: None,
                    thread_id: Some(0),
                    preserve_focus_hint: None,
                    text: None,
                    all_threads_stopped: None,
                    hit_breakpoint_ids: None,
                };

                let state = self.get_state().state.clone();
                writeln!(self.log_file, "STATE: {:?}", state);

                match self.get_state().state {
                    State::Start => {
                        stopped.reason = StoppedReason::Entry;
                    }
                    State::Break(breakpoint) => {
                        stopped.reason = StoppedReason::Breakpoint;
                        stopped.hit_breakpoint_ids = Some(vec![breakpoint]);
                    }
                    State::DataBreak(breakpoint, access_type) => {
                        stopped.reason = StoppedReason::DataBreakpoint;
                        stopped.hit_breakpoint_ids = Some(vec![breakpoint]);
                    }
                    State::Finished | State::StepIn | State::StepOver(_) => {
                        stopped.reason = StoppedReason::Step;
                    }
                    State::Pause => {
                        stopped.reason = StoppedReason::Pause;
                    }
                    _ => unreachable!("Unexpected state"),
                };
                block_on(self.send_event(EventBody::Stopped(stopped)));
            }

            writeln!(self.log_file, "  wait for command");
            writeln!(self.log_file, "stack: {:?}", env.call_stack.stack);

            let source = Source {
                name: Some(env.contract_context.contract_identifier.to_string()),
                path: Some(
                    self.contract_id_to_path[&env.contract_context.contract_identifier].clone(),
                ),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            };

            // Find the current function scope, ignoring builtin functions.
            let mut current_function = None;
            for function in env.call_stack.stack.iter().rev() {
                if !function.identifier.starts_with("_native_:") {
                    current_function = Some(function);
                    break;
                }
            }
            if let Some(current_function) = current_function {
                if let Some(stack_top) = self.stack_frames.get_mut(current_function) {
                    stack_top.stack_frame.line = expr.span.start_line;
                    stack_top.stack_frame.column = expr.span.start_column;
                    stack_top.stack_frame.end_line = Some(expr.span.end_line);
                    stack_top.stack_frame.end_column = Some(expr.span.end_column);

                    // FIXME: update the scopes here
                } else {
                    self.stack_frames.insert(
                        current_function.clone(),
                        Frame {
                            stack_frame: StackFrame {
                                id: env.call_stack.stack.len() as i32,
                                name: current_function.identifier.clone(),
                                source: Some(source.clone()),
                                line: expr.span.start_line,
                                column: expr.span.start_column,
                                end_line: Some(expr.span.end_line),
                                end_column: Some(expr.span.end_column),
                                can_restart: None,
                                instruction_pointer_reference: None,
                                module_id: None,
                                presentation_hint: Some(PresentationHint::Normal),
                            },
                            scopes: Vec::new(),
                        },
                    );
                }
            }

            // Save the current state, which may be needed to respond to incoming requests

            let scopes = vec![Scope {
                name: "Arguments".to_string(), // FIXME
                presentation_hint: Some(PresentationHint::Arguments),
                variables_reference: 0,
                named_variables: Some(context.variables.len()),
                indexed_variables: None,
                expensive: false,
                source: Some(source.clone()),
                line: None,
                column: None,
                end_line: None,
                end_column: None,
            }];
            self.current = Some(Current {
                source,
                span: expr.span.clone(),
                expr_id: expr.id,
                stack: env.call_stack.stack.clone(),
                scopes,
            });

            let mut proceed = false;
            while !proceed {
                proceed = match block_on(self.wait_for_command()) {
                    Ok(proceed) => proceed,
                    Err(e) => {
                        writeln!(self.log_file, "  ERROR: {}", e);
                        false
                    }
                };
            }
            self.current = None;
        } else {
            // TODO: If there is already a message waiting, process it before continuing.

            writeln!(self.log_file, "  continue");
        }
    }

    fn finish_eval(
        &mut self,
        env: &mut Environment,
        context: &LocalContext,
        expr: &SymbolicExpression,
        res: &Result<Value, Error>,
    ) {
        writeln!(self.log_file, "in finish_eval: {}", expr.id);
        if self.get_state().finish_eval(env, context, expr, res) {}
    }
}

pub fn create_basic_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(32)
        .build()
        .unwrap()
}

pub fn block_on<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let rt = create_basic_runtime();
    rt.block_on(future)
}
