
use crate::repl::Session;

use std::io::{Write, stdout, stdin};
use ansi_term::{Style, Colour};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Terminal {
    session: Session,
}

impl Terminal {
    pub fn new() -> Terminal {
        Terminal {
            session: Session::new()
        }
    }

    pub fn start(&mut self) {

        let light_green = Colour::Green.bold();
        let light_red = Colour::Red.bold();
        let light_black = Colour::Black.bold();

        println!("{}", light_green.paint("clarity-repl v1.0"));
        println!("{}", light_black.paint("Enter \".help\" for usage hints."));
        println!("{}", light_black.paint("Connected to a transient in-memory database."));

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
                },
                Err(ReadlineError::Interrupted) => {
                    ctrl_c_acc += 1;
                    if ctrl_c_acc == 2 {
                        break
                    } else {
                        println!("Hit CTRL-C a second time to quit.");
                    }
                },
                Err(ReadlineError::Eof) => {
                    println!("CTRL-D");
                    break
                },
                Err(err) => {
                    println!("Error: {:?}", err);
                    break
                }
            }
        }
        editor.save_history("history.txt").unwrap();
    }
}
