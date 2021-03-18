use super::ClarityInterpreter;
use crate::clarity::diagnostic::Diagnostic;
use crate::clarity::docs::{make_api_reference, make_define_reference, make_keyword_reference};
use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::types::{PrincipalData, StandardPrincipalData, QualifiedContractIdentifier};
use crate::clarity::util::StacksAddress;
use crate::clarity::variables::NativeVariables;
use ansi_term::{Colour, Style};
use std::collections::{HashMap, VecDeque};
use prettytable::{Table, Row, Cell};

use super::SessionSettings;

enum Command {
    LoadLocalContract(String),
    LoadDeployContract(String),
    UnloadContract(String),
    ExecuteSnippet(String),
    OpenSession,
    CloseSession,
}

#[derive(Clone, Debug)]
pub struct Session {
    session_id: u32,
    started_at: u32,
    settings: SessionSettings,
    contracts: Vec<(String, String)>,
    interpreter: ClarityInterpreter,
    api_reference: HashMap<String, String>,
}

impl Session {
    pub fn new(settings: SessionSettings) -> Session {
        let tx_sender = {
            let address = match settings.initial_deployer {
                Some(ref entry) => entry.address.clone(),
                None => format!("{}", StacksAddress::burn_address(false))
            };
            PrincipalData::parse_standard_principal(&address)
                .expect("Unable to parse deployer's address")
        };

        Session {
            session_id: 0,
            started_at: 0,
            settings,
            contracts: Vec::new(),
            interpreter: ClarityInterpreter::new(tx_sender),
            api_reference: build_api_reference(),
        }
    }

    pub fn start(&mut self) -> String {
        let mut output = Vec::<String>::new();
        let light_green = Colour::Green.bold();
        let light_red = Colour::Red.bold();
        let light_blue = Colour::Cyan.bold();
        let light_black = Colour::Black.bold();

        if self.settings.initial_contracts.len() > 0 {
            let mut initial_contracts = self.settings.initial_contracts.clone();
            for contract in initial_contracts.drain(..) {
                match self.formatted_interpretation(contract.code, contract.name) {
                    Ok(_) => {},
                    Err(ref mut result) => output.append(result),
                };
            }
            output.push(format!("{}", light_blue.paint("Initialized contracts")));
            self.get_contracts(&mut output);
        }

        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                let recipient = match PrincipalData::parse(&account.address) {
                    Ok(recipient) => recipient,
                    _ => {
                        output.push(format!(
                            "{}",
                            light_red.paint("Unable to parse address to credit")
                        ));
                        continue;
                    }
                };

                match self
                    .interpreter
                    .credit_stx_balance(recipient, account.balance)
                {
                    Ok(_) => {},
                    Err(err) => output.push(format!("{}", light_red.paint(err))),
                };
            }
            output.push(format!("{}", light_blue.paint("Initialized balances")));
            self.get_accounts(&mut output);
        }

        output.join("\n")
    }

    pub fn handle_command(&mut self, command: &str) -> Vec<String> {
        let mut output = Vec::<String>::new();
        match command {
            "::help" => self.display_help(&mut output),
            cmd if cmd.starts_with("::list_functions") => self.display_functions(&mut output),
            cmd if cmd.starts_with("::describe_function") => self.display_doc(&mut output, cmd),
            cmd if cmd.starts_with("::mint_stx") => self.mint_stx(&mut output, cmd),
            cmd if cmd.starts_with("::set_tx_sender") => self.parse_and_set_tx_sender(&mut output, cmd),
            cmd if cmd.starts_with("::get_accounts") => self.get_accounts(&mut output),
            cmd if cmd.starts_with("::get_contracts") => self.get_contracts(&mut output),
            cmd if cmd.starts_with("::get_block_height") => self.get_block_height(&mut output),
            cmd if cmd.starts_with("::advance_chain_tip") => self.parse_and_advance_chain_tip(&mut output, cmd),

            snippet => {
                let mut result = match self.formatted_interpretation(snippet.to_string(), None) {
                    Ok(result) => result,
                    Err(result) => result,
                };
                output.append(&mut result);
            }
        }
        output
    }

    pub fn formatted_interpretation(
        &mut self,
        snippet: String,
        name: Option<String>,
    ) -> Result<Vec<String>, Vec<String>> {
        let light_green = Colour::Green.bold();
        let light_red = Colour::Red.bold();
        let light_black = Colour::Black.bold();

        let result = self.interpret(snippet.to_string(), name);
        let mut output = Vec::<String>::new();

        match result {
            Ok((contract_name, result)) => {
                if let Some(contract_name) = contract_name {
                    let snippet = format!("→ .{} contract successfully stored. Use (contract-call? ...) for invoking the public functions:", contract_name.clone());
                    output.push(format!("{}", light_green.paint(snippet)));
                }
                output.push(format!("{}", light_green.paint(result)));
                Ok(output)
            }
            Err((message, diagnostic)) => {
                output.push(format!("{}", light_red.paint(message)));
                if let Some(diagnostic) = diagnostic {
                    if diagnostic.spans.len() > 0 {
                        let lines = snippet.lines();
                        let mut formatted_lines: Vec<String> =
                            lines.map(|l| l.to_string()).collect();
                        for span in diagnostic.spans {
                            let first_line = span.start_line.saturating_sub(1) as usize;
                            let last_line = span.end_line.saturating_sub(1) as usize;
                            let mut pass = vec![];

                            for (line_index, line) in formatted_lines.iter().enumerate() {
                                if line == "" {
                                    pass.push(line.clone());
                                    continue;
                                }
                                if line_index >= first_line && line_index <= last_line {
                                    let (begin, end) =
                                        match (line_index == first_line, line_index == last_line) {
                                            (true, true) => (
                                                span.start_column.saturating_sub(1) as usize ,
                                                span.end_column.saturating_sub(1) as usize,
                                            ), // One line
                                            (true, false) => {
                                                (span.start_column.saturating_sub(1) as usize, line.len().saturating_sub(1))
                                            } // Multiline, first line
                                            (false, false) => (0, line.len().saturating_sub(1)), // Multiline, in between
                                            (false, true) => (0, span.end_column.saturating_sub(1) as usize), // Multiline, last line
                                        };

                                    let error_style = light_red.underline();
                                    let formatted_line = format!(
                                        "{}{}{}",
                                        &line[..begin],
                                        error_style.paint(&line[begin..=end]),
                                        &line[(end + 1)..]
                                    );
                                    pass.push(formatted_line);
                                } else {
                                    pass.push(line.clone());
                                }
                            }
                            formatted_lines = pass;
                        }
                        output.append(&mut formatted_lines);
                    }
                }
                Err(output)
            }
        }
    }

    pub fn interpret(
        &mut self,
        snippet: String,
        name: Option<String>,
    ) -> Result<(Option<String>, String), (String, Option<Diagnostic>)> {
        let contract_name = match name {
            Some(name) => name,
            None => format!("contract-{}", self.contracts.len()),
        };
        let tx_sender = self.interpreter.get_tx_sender().to_address();
        let contract_identifier = {
            let id = format!("{}.{}", tx_sender, contract_name);
            QualifiedContractIdentifier::parse(&id).unwrap()
        };

        match self.interpreter.run(snippet, contract_identifier.clone()) {
            Ok((contract_saved, res)) => {
                if contract_saved {
                    self.contracts.push((contract_identifier.to_string(), res.clone()));
                    Ok((Some(contract_name), res))
                } else {
                    Ok((None, res))
                }
            }
            Err(res) => Err(res),
        }
    }

    pub fn lookup_api_reference(&self, keyword: &str) -> Option<&String> {
        self.api_reference.get(keyword)
    }

    pub fn get_api_reference_index(&self) -> Vec<String> {
        let mut keys = self
            .api_reference
            .iter()
            .map(|(k, _)| k.to_string())
            .collect::<Vec<String>>();
        keys.sort();
        keys
    }

    fn display_help(&self, output: &mut Vec<String>) {
        let help_colour = Colour::Yellow;
        let coming_soon_colour = Colour::Black.bold();
        output.push(format!(
            "{}",
            help_colour.paint("::help\t\t\t\t\tDisplay help")
        ));
        output.push(format!(
            "{}",
            help_colour
                .paint("::list_functions\t\t\tDisplay all the native functions available in clarity")
        ));
        output.push(format!(
            "{}",
            help_colour.paint(
                "::describe_function <function>\t\tDisplay documentation for a given native function fn-name"
            )
        ));
        output.push(format!(
            "{}",
            help_colour
                .paint("::mint_stx <principal> <amount>\t\tMint STX balance for a given principal")
        ));
        output.push(format!(
            "{}",
            help_colour
                .paint("::set_tx_sender <principal>\t\tSet tx-sender variable to principal")
        ));
        output.push(format!(
            "{}",
            help_colour
                .paint("::get_accounts\t\t\t\tGet genesis accounts")
        ));
        output.push(format!(
            "{}",
            help_colour
                .paint("::get_contracts\t\t\t\tGet contracts")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::get_block_height\t\t\tGet current block height")
        ));
        output.push(format!(
            "{}",
            help_colour
                .paint("::advance_chain_tip <count>\t\tSimulate mining of <count> blocks")
        ));
    }

    fn parse_and_advance_chain_tip(&mut self, output: &mut Vec<String>, command: &str) {
        let args: Vec<_> = command.split(' ').collect();
        let light_green = Colour::Green.bold();
        let light_red = Colour::Red.bold();

        if args.len() != 2 {
            output.push(format!(
                "{}",
                light_red.paint("Usage: ::advance_chain_tip <count>")
            ));
            return;
        }

        let count = match args[1].parse::<u32>() {
            Ok(count) => count,
            _ => {
                output.push(format!(
                    "{}",
                    light_red.paint("Unable to parse count")
                ));
                return;
            }
        };

        let new_height = self.advance_chain_tip(count);
        output.push(format!("{}", light_green.paint(format!("{} blocks simulated, new height: {}", count, new_height))));
    }

    pub fn advance_chain_tip(
        &mut self,
        count: u32,
    ) -> u32 {
        self.interpreter.advance_chain_tip(count)
    }

    fn parse_and_set_tx_sender(&mut self, output: &mut Vec<String>, command: &str) {
        let args: Vec<_> = command.split(' ').collect();
        let light_green = Colour::Green.bold();
        let light_red = Colour::Red.bold();

        if args.len() != 2 {
            output.push(format!(
                "{}",
                light_red.paint("Usage: ::set_tx_sender <address>")
            ));
            return;
        }

        let tx_sender = match PrincipalData::parse_standard_principal(&args[1]) {
            Ok(address) => address,
            _ => {
                output.push(format!(
                    "{}",
                    light_red.paint("Unable to parse the address")
                ));
                return;
            }
        };

        self.set_tx_sender(tx_sender.to_address());
        output.push(format!("{}", light_green.paint(format!("tx-sender switched to {}", tx_sender))));
    }

    pub fn set_tx_sender(&mut self, address: String) {
        let tx_sender = PrincipalData::parse_standard_principal(&address)
            .expect("Unable to parse address");
        self.interpreter.set_tx_sender(tx_sender)
    }

    pub fn get_tx_sender(&self) -> String {
        self.interpreter.get_tx_sender().to_address()
    }

    fn get_block_height(&mut self, output:&mut Vec<String>) {
        let light_green = Colour::Green.bold();
        let height = self.interpreter.get_block_height();
        output.push(format!("{}", light_green.paint(format!("Current height: {}", height))));
    }
    
    fn get_accounts(&mut self, output:&mut Vec<String>) {
        if self.settings.initial_accounts.len() > 0 {
            let mut table = Table::new();
            table.add_row(row!["Name", "Address", "Balance"]);
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                table.add_row(Row::new(vec![
                    Cell::new(&account.name),
                    Cell::new(&account.address),
                    Cell::new(&format!("{}", account.balance))]));
            }
            output.push(format!("{}", table));
        }
    }

    fn get_contracts(&mut self, output:&mut Vec<String>) {
        if self.settings.initial_accounts.len() > 0 {
            let mut table = Table::new();
            table.add_row(row!["Contract identifier", "Public functions"]);
            let mut initial_contracts = self.contracts.clone();
            for contract in initial_contracts.drain(..) {
                table.add_row(Row::new(vec![
                    Cell::new(&contract.0),
                    Cell::new(&contract.1)]));
            }
            output.push(format!("{}", table));
        }
    }

    fn mint_stx(&mut self, output: &mut Vec<String>, command: &str) {
        let args: Vec<_> = command.split(' ').collect();
        let light_green = Colour::Green.bold();
        let light_red = Colour::Red.bold();

        if args.len() != 3 {
            output.push(format!(
                "{}",
                light_red.paint("Usage: ::mint_stx <recipient address> <amount>")
            ));
            return;
        }

        let recipient = match PrincipalData::parse(&args[1]) {
            Ok(address) => address,
            _ => {
                output.push(format!(
                    "{}",
                    light_red.paint("Unable to parse the address")
                ));
                return;
            }
        };

        let amount: u64 = match args[2].parse() {
            Ok(recipient) => recipient,
            _ => {
                output.push(format!(
                    "{}",
                    light_red.paint("Unable to parse the balance")
                ));
                return;
            }
        };

        match self.interpreter.credit_stx_balance(recipient, amount) {
            Ok(msg) => output.push(format!("{}", light_green.paint(msg))),
            Err(err) => output.push(format!("{}", light_red.paint(err))),
        };
    }

    fn display_functions(&self, output: &mut Vec<String>) {
        let help_colour = Colour::Yellow;
        let api_reference_index = self.get_api_reference_index();
        output.push(format!(
            "{}",
            help_colour.paint(api_reference_index.join("\n"))
        ));
    }

    fn display_doc(&self, output: &mut Vec<String>, command: &str) {
        let help_colour = Colour::Yellow;
        let help_accent_colour = Colour::Yellow.bold();
        let keyword = {
            let mut s = command.to_string();
            s = s.replace("::doc", "");
            s = s.replace(" ", "");
            s
        };
        let result = match self.lookup_api_reference(&keyword) {
            Some(doc) => format!("{}", help_colour.paint(doc)),
            None => format!("{}", help_colour.paint("Function unknown")),
        };
        output.push(result);
    }
}

fn build_api_reference() -> HashMap<String, String> {
    let mut api_reference = HashMap::new();
    for func in NativeFunctions::ALL.iter() {
        let api = make_api_reference(&func);
        let description = {
            let mut s = api.description.to_string();
            s = s.replace("\n", " ");
            s
        };
        let doc = format!(
            "Usage\n{}\n\nDescription\n{}\n\nExamples\n{}",
            api.signature, description, api.example
        );
        api_reference.insert(api.name, doc);
    }

    for func in DefineFunctions::ALL.iter() {
        let api = make_define_reference(&func);
        let description = {
            let mut s = api.description.to_string();
            s = s.replace("\n", " ");
            s
        };
        let doc = format!(
            "Usage\n{}\n\nDescription\n{}\n\nExamples\n{}",
            api.signature, description, api.example
        );
        api_reference.insert(api.name, doc);
    }

    for func in NativeVariables::ALL.iter() {
        let api = make_keyword_reference(&func);
        let description = {
            let mut s = api.description.to_string();
            s = s.replace("\n", " ");
            s
        };
        let doc = format!("Description\n{}\n\nExamples\n{}", description, api.example);
        api_reference.insert(api.name.to_string(), doc);
    }
    api_reference
}
