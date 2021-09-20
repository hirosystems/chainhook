use std::io::{stdin, stdout, Write};
use std::str::FromStr;

use ansi_term::{Colour, Style};
use prettytable::{format, Table};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use serde::Serialize;

use crate::repl::{self, CommandResult};
use crate::repl::{settings::SessionSettings, OutputMode, ReplCommand, Session};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const HISTORY_FILE: Option<&'static str> = option_env!("CLARITY_REPL_HISTORY_FILE");

struct Prompt {
    promt: String,
}

impl Prompt {
    pub fn prompty(&self) -> &str {
        &*self.promt
    }
}

fn complete_input(str: &str) -> Result<Option<char>, (char, char)> {
    let mut brackets = vec![];
    for character in str.chars() {
        match character {
            '(' | '{' => brackets.push(character),
            ')' | '}' => match (brackets.pop(), character) {
                (Some('('), '}') => return Err((')', '}')),
                (Some('{'), ')') => return Err(('}', ')')),
                _ => {}
            },
            _ => {}
        }
    }
    match brackets.last() {
        Some(char) => Ok(Some(*char)),
        _ => Ok(None),
    }
}

pub struct Terminal {
    pub session: Session,
}

impl Terminal {
    pub fn new(session_settings: SessionSettings) -> Terminal {
        Terminal {
            session: Session::new(session_settings),
        }
    }

    pub fn start(&mut self) {
        println!("{}", green!(format!("clarity-repl v{}", VERSION.unwrap())));
        println!("{}", black!("Enter \"::help\" for usage hints."));
        println!("{}", black!("Connected to a transient in-memory database."));

        let _ = self.session.start();
        println!("\n{}", blue!("Contracts"));
        let comm = ReplCommand::GetContracts
            .execute(&mut self.session)
            .and_then(|command_result| command_result.map(self.session.output_mode))
            .map_err(|e| format!("{}", e));
        match comm {
            Ok(res) => println!("{}", res),
            Err(e) => println!("{}", Colour::Red.paint(e)),
        }
        println!("{}", blue!("Initialized balances"));
        let comm = ReplCommand::GetAssetsMaps
            .execute(&mut self.session)
            .and_then(|command_result| command_result.map(self.session.output_mode))
            .map_err(|e| format!("{}", e));
        match comm {
            Ok(res) => println!("{}", res),
            Err(e) => println!("{}", Colour::Red.paint(e)),
        }

        let mut editor = Editor::<()>::new();
        let mut ctrl_c_acc = 0;
        let mut input_buffer = vec![];
        let mut prompt = ">> ".to_owned();
        loop {
            let readline = editor.readline(prompt.as_str());
            match readline {
                Ok(command) => {
                    ctrl_c_acc = 0;
                    input_buffer.push(command);
                    let input = input_buffer.join("\n");
                    match complete_input(&input) {
                        Ok(None) => {
                            let repl_command = ReplCommand::from_str(&input);

                            let res = repl_command
                                .and_then(|command| Ok(command.execute(&mut self.session)))
                                .and_then(|c| c?.map(self.session.output_mode))
                                .map_err(|e| format!("{}", e));

                            match res {
                                Ok(res) => println!("{}", res),
                                Err(e) => println!("{}", Colour::Red.paint(e)),
                            }
                            prompt = ">> ".to_owned();
                            editor.add_history_entry(&input);
                            input_buffer.clear();
                        }
                        Ok(Some(str)) => {
                            prompt = format!("{}.. ", str);
                        }
                        Err((expected, got)) => {
                            println!("Error: expected closing {}, got {}", expected, got);
                            input_buffer.pop();
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    ctrl_c_acc += 1;
                    if ctrl_c_acc == 2 {
                        break;
                    } else {
                        println!("{}", yellow!("Hit CTRL-C a second time to quit."));
                    }
                }
                Err(ReadlineError::Eof) => {
                    println!("CTRL-D");
                    break;
                }
                Err(err) => {
                    println!("Error: {:?}", err);
                    break;
                }
            }
        }
        editor
            .save_history(HISTORY_FILE.unwrap_or("history.txt"))
            .unwrap();
    }
}
