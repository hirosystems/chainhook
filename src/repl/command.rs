use std::{
    any::Any,
    collections::HashMap,
    convert::TryFrom,
    fmt::{Debug, Display},
    num::ParseIntError,
    str::FromStr,
};

use ansi_term::{
    Colour::{Blue, Green, Yellow, Red},
    Style,
};
use itertools::Itertools;
use prettytable::{
    format::{FormatBuilder, TableFormat},
    *,
};
use serde::Serialize;

use anyhow::{Context, Result};
use serde_json::map::IntoIter;
use thiserror::Error;

use crate::{
    clarity::{
        self,
        types::{PrincipalData, StandardPrincipalData},
    },
    repl::{session::build_api_reference, OutputMode},
};

use super::Session;

#[derive(Error, Debug, Serialize)]
pub enum MintParseError {
    #[error("Unable to parse the balance: {0}")]
    BalanceParseError(String),
    #[error("Unable to parse the address")]
    AddressParseError,
}

impl From<ParseIntError> for MintParseError {
    fn from(p: ParseIntError) -> Self {
        Self::BalanceParseError(p.to_string())
    }
}

impl From<clarity::errors::Error> for MintParseError {
    fn from(_: clarity::errors::Error) -> Self {
        Self::AddressParseError
    }
}

#[derive(Error, Debug, Serialize)]
pub enum CommandError {
    #[error("Command can't be parsed: {0}")]
    CommandParseError(String),
    #[error("Command not found: ::{0}")]
    CommandNotFound(String),
    #[error("Function not found: {0}")]
    FunctionNotFound(String),
    #[error("Usage: {0}")]
    CommandUsageError(Usage),
    #[error("Can not mint: {0}")]
    MintParseError(MintParseError),
    #[error("Can not parse: {0}")]
    ParseIntError(String),
}

impl From<ReplCommand> for CommandError {
    fn from(v: ReplCommand) -> Self {
        match v {
            ReplCommand::AdvanceChainTip(_) => Self::CommandUsageError(Usage::AdvanceChainTip),
            ReplCommand::DescribeFunction(_) => Self::CommandUsageError(Usage::DescribeFunction),
            ReplCommand::GetCosts(_) => Self::CommandUsageError(Usage::DescribeFunction),
            ReplCommand::SetTxSender(_) => Self::CommandUsageError(Usage::SetTxSender),
            ReplCommand::MintStx(_, _) => Self::CommandUsageError(Usage::MintStx),
            _ => unreachable!(),
        }
    }
}

#[derive(Error, Debug)]
pub enum CommandExecuteError {
    #[error("Can not parse Principal {0}")]
    PrincipalParseError(MintParseError),
}

impl From<ParseIntError> for CommandError {
    fn from(p: ParseIntError) -> Self {
        Self::ParseIntError(p.to_string())
    }
}

impl From<MintParseError> for CommandError {
    fn from(m: MintParseError) -> Self {
        Self::MintParseError(m)
    }
}

impl From<clarity::errors::Error> for CommandError {
    fn from(_: clarity::errors::Error) -> Self {
        Self::MintParseError(MintParseError::AddressParseError)
    }
}

#[derive(Debug, Serialize)]
pub enum ReplCommand {
    Help,
    SwitchOutputMode,
    ListFunctions(bool),
    ListTests,
    DescribeFunction(String),
    MintStx(PrincipalData, u64),
    SetTxSender(StandardPrincipalData),
    GetAssetsMaps,
    GetCosts(String),
    GetContracts,
    GetBlockHeight,
    AdvanceChainTip(u32),
    ExecuteSnippet(String),
}

#[derive(Debug, Serialize)]
pub enum Usage {
    AdvanceChainTip,
    SetTxSender,
    DescribeFunction,
    MintStx,
    GetCosts,
}

impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::AdvanceChainTip => "::advance_chain_tip <count>",
            Self::SetTxSender => "::set_tx_sender <address>",
            Self::DescribeFunction => "::describe_function <function>",
            Self::MintStx => "::mint_stx <recipient address> <amount>",
            Self::GetCosts => "::get_costs <expression>",
        };

        write!(f, "{}", output)
    }
}

#[derive(Serialize)]
pub enum CommandResult {
    Ok(String),
    Table(OutputTable),
    List(Vec<String>),
    // String(String),
    Description(String),
    Error(CommandError),
}
impl CommandResult {
    pub fn map(&self, output_mode: OutputMode) -> Result<String, CommandError> {
        Ok(match output_mode {
            OutputMode::Console => match self {
                CommandResult::Table(output_table) => {
                    Table::from(output_table.clone()).to_string()
                }
                CommandResult::List(list) => {
                    Yellow.paint(list.join("\n")).to_string()
                }
                CommandResult::Description(s) => {
                    Yellow.paint(s).to_string()
                }
                CommandResult::Error(e) => {
                    Red.paint(format!("{}", e)).to_string()
                }
                CommandResult::Ok(msg) => {
                    Green.paint(msg).to_string()
                }
            },
            OutputMode::Json => format!(
                "{}",
                serde_json::to_string_pretty(&self).unwrap()
            ),
        })
    }
}
impl ReplCommand {
    pub fn execute(&self, session: &mut Session) -> CommandResult {
        match self {
            ReplCommand::Help => CommandResult::Table(help_table()),
            ReplCommand::SwitchOutputMode => {
                session.output_mode = OutputMode::switch(&session.output_mode);
                CommandResult::Ok(format!("output mode: {:?}", session.output_mode))
            }
            ReplCommand::ListFunctions(detailed) => match detailed {
                false => CommandResult::List(list_functions()),
                true => CommandResult::Table(functions_table().into()),
            },
            ReplCommand::ListTests => CommandResult::List(list_tests()),
            ReplCommand::DescribeFunction(f) => match session.lookup_api_reference(&f) {
                Some(desc) => CommandResult::Description(desc.to_string()),
                None => CommandResult::Error(CommandError::FunctionNotFound(f.clone())),
            },
            ReplCommand::MintStx(recipient, amount) => match session
                .interpreter
                .credit_stx_balance(recipient.clone(), *amount)
            {
                Ok(msg) => CommandResult::Ok(msg),
                Err(err) => CommandResult::Error(CommandError::CommandParseError(err)),
            },
            ReplCommand::SetTxSender(principal_data) => {
                session.set_tx_sender(principal_data.clone());
                CommandResult::Ok(format!("tx-sender switched to {}", principal_data))
            }
            ReplCommand::GetAssetsMaps => {
                let table = get_assets_maps(&session);
                CommandResult::Table(table)
            }
            ReplCommand::GetCosts(snippet) => {
                CommandResult::Table(get_costs(snippet.clone(), session))
            }
            ReplCommand::GetContracts => CommandResult::Table(get_contracts(&session)),
            ReplCommand::GetBlockHeight => {
                CommandResult::Ok(session.interpreter.get_block_height().to_string())
            }
            ReplCommand::AdvanceChainTip(count) => {
                CommandResult::Ok(session.advance_chain_tip(*count).to_string())
            }
            ReplCommand::ExecuteSnippet(s) => {
                match session.formatted_interpretation(s.to_string(), None, false, None) {
                    Ok((result, _)) => CommandResult::List(result),
                    Err(result) => CommandResult::List(result),
                }
            }
        }
    }
}

impl FromStr for ReplCommand {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("::") {
            // ignore `::` at the start
            let mut args: Vec<&str> = s[2..].split(' ').collect();
            let command_str = args.remove(0);
            // println!("args: {:?}, len: {}", args, args.len());
            let command = match command_str {
                "help" => Ok(Self::Help),
                "s" | "switch_output_mode" => Ok(Self::SwitchOutputMode),
                "list_functions" => Ok(Self::ListFunctions(false)),
                "list_functions_detailed" => Ok(Self::ListFunctions(true)),
                "list_tests" => Ok(Self::ListTests),
                "get_assets_maps" => Ok(Self::GetAssetsMaps),
                "get_costs" => match args.is_empty() {
                    false => Ok(Self::GetCosts(args.join(" "))),
                    true => Err(CommandError::CommandUsageError(Usage::GetCosts)),
                },
                "get_contracts" => Ok(Self::GetContracts),
                "get_block_height" => Ok(Self::GetBlockHeight),
                "advance_chain_tip" => match !args.is_empty() {
                    true => Ok(Self::AdvanceChainTip(args[0].parse()?)),
                    false => Err(CommandError::CommandUsageError(Usage::AdvanceChainTip)),
                },
                "describe_function" => match args.is_empty() || args[0].is_empty() {
                    false => Ok(Self::DescribeFunction(args[0].to_owned())),
                    true => Err(CommandError::CommandUsageError(Usage::DescribeFunction)),
                },
                "mint_stx" => match args.len() == 2 {
                    true => Ok(Self::MintStx(args[0].parse()?, args[1].parse()?)),
                    false => Err(CommandError::CommandUsageError(Usage::MintStx)),
                },
                "set_tx_sender" => match !args.is_empty() {
                    true => Ok(Self::SetTxSender(PrincipalData::parse_standard_principal(
                        args[0],
                    )?)),
                    false => Err(CommandError::CommandUsageError(Usage::SetTxSender)),
                },

                _ => Err(CommandError::CommandNotFound(command_str.to_owned())),
            };
            return command;
        } else {
            Ok(Self::ExecuteSnippet(s.to_string()))
        }
    }
}

/// An owned printable table
#[derive(Serialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct OutputTable {
    #[serde(skip_serializing)]
    format: TableFormat,
    titles: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
}

impl OutputTable {
    pub fn set_format(&mut self, format: TableFormat) {
        self.format = format;
    }

    pub fn set_titles(&mut self, titles: Vec<String>) {
        self.titles = Some(titles);
    }
}

impl From<Vec<Vec<String>>> for OutputTable {
    fn from(v: Vec<Vec<String>>) -> Self {
        OutputTable {
            format: TableFormat::new(),
            titles: None,
            rows: v,
        }
    }
}

impl From<OutputTable> for Table {
    fn from(output_table: OutputTable) -> Self {
        let tbl = output_table
            .rows
            .iter()
            .map(|v| {
                v.iter()
                    .enumerate()
                    .map(|(i, x)| {
                        match i {
                            0 => Cell::new(&Blue.paint(x).to_string()), //Paint first cloumn different
                            _ => Cell::new(&Yellow.paint(x).to_string()),
                        }
                    })
                    .collect::<Row>()
            })
            .collect::<Vec<Row>>()
            .into();
        let mut table = Table::init(tbl);
        if let Some(titles) = output_table.titles {
            table.set_titles(Row::new(
                titles
                    .iter()
                    .map(|x| Cell::new(&Green.bold().underline().paint(x).to_string())) // Paint titles
                    .collect_vec(),
            ))
        }

        table.set_format(output_table.format);
        table
    }
}

fn help_table() -> OutputTable {
    let help = vec![
        vec!["::help", "Display help"],
        vec![
            "::list_functions",
            "Display all the native functions available in clarity",
        ],
        vec!["::list_tests", "lists tests"],
        vec![
            "::describe_function <function>",
            "Display documentation for a given native function fn-name",
        ],
        vec![
            "::mint_stx <principal> <amount>",
            "Mint STX balance for a given principal",
        ],
        vec![
            "::set_tx_sender <principal>",
            "Set tx-sender variable to principal",
        ],
        vec!["::get_assets_maps", "Get assets maps for active accounts"],
        vec!["::get_costs <expr>", "Display the cost analysis"],
        vec!["::get_contracts", "Get contracts"],
        vec!["::get_block_height", "Get current block height"],
        vec![
            "::advance_chain_tip <count>",
            "Simulate mining of <count> blocks",
        ],
    ];

    let rows = help
        .iter()
        .map(|v| v.iter().cloned().map_into::<String>().collect_vec())
        .collect();
    let titles = vec!["Command".to_string(), "Description".to_string()];
    let format = FormatBuilder::new().indent(3).padding(2, 3).build();

    let table = OutputTable {
        format,
        titles: Some(titles.into()),
        rows,
    };
    table
}

fn list_functions() -> Vec<String> {
    // let mut output: Vec<String> = Vec::new();
    // let help_colour = Colour::Yellow;
    let api_reference_index = build_api_reference();
    let mut keys = api_reference_index
        .iter()
        .map(|(k, _)| k.to_string())
        .collect::<Vec<String>>();
    keys.sort();
    keys
}

fn functions_table() -> Vec<Vec<String>> {
    let api_reference_index = build_api_reference();
    let table = api_reference_index
        .iter()
        .map(|(a, b)| vec![a.to_string(), b.to_string()])
        .collect::<Vec<Vec<String>>>();
    table
}

fn list_tests() -> Vec<String> {
    let mut res = Vec::new();
    let dir = std::fs::read_dir("tests").unwrap();
    for entry in dir {
        let ent = format!("{}", entry.unwrap().path().display());
        res.push(ent);
    }
    res
}

fn get_assets_maps(session: &Session) -> OutputTable {
    let accounts = session.interpreter.get_accounts();
    let tokens = session.interpreter.get_tokens();
    let mut headers = vec!["Address".to_string()];
    headers.extend(tokens.iter().cloned());

    let accounts = accounts
        .iter()
        .map(|acc| {
            let address = session
                .settings
                .initial_accounts
                .iter()
                .find(|a| &a.address == acc)
                .and_then(|ac| Some(format!("{} ({})", acc, ac.name)))
                .unwrap_or(acc.to_owned());
            let balances = tokens
                .iter()
                .map(|token| {
                    session
                        .interpreter
                        .get_balance_for_account(acc, token)
                        .to_string()
                })
                .collect::<Vec<String>>();
            let balance = Balance { address, balances };
            balance.into_iter().collect()
        })
        .collect();
    let func_style = Style::new().fg(Blue);
    let desc_style = Style::new().fg(Yellow);
    let format = prettytable::format::consts::FORMAT_BOX_CHARS.clone();
    // FormatBuilder::new().indent(3).padding(2, 3).build();

    OutputTable {
        format,
        titles: Some(headers),
        rows: accounts,
    }
}

#[derive(Serialize, Debug)]
pub struct Balance {
    address: String,
    #[serde(flatten)]
    balances: Vec<String>,
}

impl IntoIterator for Balance {
    type Item = String;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut vec = vec![self.address.clone()];
        vec.extend(self.balances.into_iter());
        vec.into_iter()
    }
}

fn get_contracts(session: &Session) -> OutputTable {
    let mut rows = Vec::new();
    let contracts = session.contracts.clone();
    for (contract_id, methods) in contracts.iter() {
        if !contract_id.ends_with(".pox")
            && !contract_id.ends_with(".bns")
            && !contract_id.ends_with(".costs")
        {
            let mut formatted_methods = vec![];
            for (method_name, method_args) in methods.iter() {
                let formatted_args = if method_args.len() == 0 {
                    format!("")
                } else if method_args.len() == 1 {
                    format!(" {}", method_args.join(" "))
                } else {
                    format!("\n    {}", method_args.join("\n    "))
                };
                formatted_methods.push(format!("({}{})", method_name, formatted_args));
            }
            let formatted_spec = format!("{}", formatted_methods.join("\n"));
            rows.push(vec![contract_id.clone(), formatted_spec]);
        }
    }

    let titles = vec![
        "Contract identifier".to_string(),
        "Public functions".to_string(),
    ];

    let format = format::consts::FORMAT_BOX_CHARS.clone();

    OutputTable {
        format,
        titles: Some(titles.into()),
        rows,
    }
}

pub fn get_costs(snippet: String, session: &mut Session) -> OutputTable {
    let rows;
    let (result, cost) = match session.formatted_interpretation(snippet, None, true, None) {
        Ok((output, result)) => (output, result.cost.clone()),
        Err(output) => (output, None),
    };

    if let Some(cost) = cost {
        let headers = vec!["".to_string(), "Consumed".to_string(), "Limit".to_string()];
        let first_col = vec![
            "Runtime",
            "Read count",
            "Read length (bytes)",
            "Write count",
            "Write length (bytes)",
        ];

        let consumed = cost.total.to_vec();
        let limit = cost.limit.to_vec();
        rows = first_col
            .iter()
            .zip(consumed.iter())
            .zip(limit.iter())
            .map(|((a, b), c)| vec![a.to_string(), b.to_string(), c.to_string()])
            .collect();
    } else {
        rows = Vec::new()
    }

    let titles = vec!["".to_string(), "Consumed".to_string(), "Limit".to_string()];
    let mut format = prettytable::format::consts::FORMAT_BOX_CHARS.clone();
    format.indent(3);
    format.padding(2, 3);

    let table = OutputTable {
        format,
        titles: Some(titles.into()),
        rows,
    };

    table
}
