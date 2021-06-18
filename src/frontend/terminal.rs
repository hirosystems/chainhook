use crate::repl::{settings::SessionSettings, Session};

use ansi_term::{Colour, Style};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::io::{stdin, stdout, Write};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

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
    session: Session,
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

        let (res, _) = self.session.start();
        println!("{}", res);
        let mut editor = Editor::<()>::new();
        let mut ctrl_c_acc = 0;
        let mut input_buffer = String::new();
        let mut prompt = String::from(">> ");
        loop {
            let readline = editor.readline(prompt.as_str());
            match readline {
                Ok(command) => {
                    ctrl_c_acc = 0;
                    if !input_buffer.is_empty() {
                        input_buffer.push_str("\n");
                    }
                    input_buffer.push_str(&command);
                    match complete_input(&input_buffer) {
                        Ok(None) => {
                            let output = self.session.handle_command(&input_buffer);
                            for line in output {
                                println!("{}", line);
                            }
                            prompt = String::from(">> ");
                            editor.add_history_entry(input_buffer.as_str());
                            input_buffer.clear();
                        }
                        Ok(Some(str)) => {
                            prompt = format!("{}.. ", str);
                        }
                        Err((expected, got)) => {
                            println!("Error: expected closing {}, got {}", expected, got);
                            if input_buffer.len() == command.len() {
                                input_buffer.clear();
                            } else {
                                input_buffer.truncate(input_buffer.len() - command.len() - 1);
                            }
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
        editor.save_history("history.txt").unwrap();
    }
}
