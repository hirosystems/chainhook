#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

#[cfg(feature = "wasm")]
extern crate wasm_bindgen;

#[cfg(feature = "cli")]
#[macro_use]
extern crate prettytable;

#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod clarity;
pub mod repl;

#[cfg(feature = "cli")]
pub mod frontend;

#[cfg(feature = "cli")]
pub use frontend::Terminal;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn handle_command(command: &str) -> String {
    let mut session = Session::new(SessionSettings::default());
    let output_lines = session.handle_command(command);
    output_lines.join("\n").to_string()
}
