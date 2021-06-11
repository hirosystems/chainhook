use crate::repl::{settings::SessionSettings, Session};

use ansi_term::{Colour, Style};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::io::{stdin, stdout, Write};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

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
        println!(
            "{}",
            black!("Connected to a transient in-memory database.")
        );

        let (res, _) = self.session.start();
        println!("{}", res);

        let mut editor = Editor::<()>::new();
        let mut ctrl_c_acc = 0;
        loop {
            let readline = editor.readline(">> ");
            match readline {
                Ok(command) => {
                    let output = self.session.handle_command(&command);
                    for line in output {
                        println!("{}", line);
                    }
                    ctrl_c_acc = 0;
                    editor.add_history_entry(command.as_str());
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
