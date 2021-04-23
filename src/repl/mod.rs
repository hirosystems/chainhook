use std::collections::BTreeMap;
use serde_json::Value;

pub mod interpreter;
pub mod session;
pub mod settings;

pub use interpreter::ClarityInterpreter;
pub use session::Session;
pub use settings::SessionSettings;

#[derive(Default)]
pub struct ExecutionResult {
    pub contract: Option<(String, BTreeMap<String, Vec<String>>)>,
    pub result: Option<String>,
    pub events: Vec<Value>,
}
