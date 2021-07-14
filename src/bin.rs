#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
// todo(ludo): would love to eliminate these directives at some point.

#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate prettytable;

#[macro_use]
mod macros;

pub mod clarity;
pub mod contracts;
pub mod frontend;
pub mod repl;

use frontend::Terminal;
use pico_args::Arguments;
use repl::{settings, Session, SessionSettings};
use std::env;

fn main() {
    let mut args = Arguments::from_env();
    let subcommand = args.subcommand().unwrap().unwrap_or_default();

    let mut settings = SessionSettings::default();
    settings.include_boot_contracts = vec!["costs".into()];

    let mut terminal = Terminal::new(settings);
    terminal.start();
}
