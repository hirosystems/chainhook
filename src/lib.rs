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

#[macro_use]
mod macros;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod clarity;
pub mod repl;
pub mod contracts;

struct GlobalContext {
    session: Option<Session>,
}

impl GlobalContext {
    pub fn new() -> Self {
        Self {
            session: None
        }
    }   
}

static mut WASM_GLOBAL_CONTEXT: GlobalContext = GlobalContext::new();

#[cfg(feature = "cli")]
pub mod frontend;

#[cfg(feature = "cli")]
pub use frontend::Terminal;

use repl::{Session, SessionSettings};

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn handle_command(fetch_contract: &str, command: &str) -> String {

    let mut session = unsafe { match WASM_GLOBAL_CONTEXT.session.take() {
        Some(session) => session,
        None => {
            let mut settings = SessionSettings::default();
            settings.include_boot_contracts = vec!["costs".into()];
            let mut session = Session::new(settings);
            session.start();
            session
        }
    }};

    let output_lines = session.handle_command(command);

    unsafe {
        WASM_GLOBAL_CONTEXT.session = Some(session);
    }

    output_lines.join("\n").to_string()
}
