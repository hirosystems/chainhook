use super::{ClarityInterpreter, ExecutionResult};
use crate::clarity::diagnostic::Diagnostic;
use crate::clarity::docs::{make_api_reference, make_define_reference, make_keyword_reference};
use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::types::{PrincipalData, StandardPrincipalData, QualifiedContractIdentifier};
use crate::clarity::util::StacksAddress;
use crate::clarity::variables::NativeVariables;
use crate::contracts::{POX_CONTRACT, BNS_CONTRACT, COSTS_CONTRACT};
use ansi_term::{Colour, Style};
use std::collections::{HashMap, BTreeSet, VecDeque, BTreeMap};
use serde_json::Value;

#[cfg(feature = "cli")]
use prettytable::{Table, Row, Cell};

use super::SessionSettings;
use super::settings::InitialLink;

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
    contracts: BTreeMap<String, BTreeMap<String, Vec<String>>>,
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
            contracts: BTreeMap::new(),
            interpreter: ClarityInterpreter::new(tx_sender),
            api_reference: build_api_reference(),
        }
    }

    fn retrieve_contract(&mut self, link: &InitialLink) -> Result<(String, BTreeSet<String>), String> {
        let contract_id = &link.contract_id;
        let components: Vec<&str> = contract_id.split('.').collect();
        let contract_deployer = components.first().expect("");
        let contract_name = components.last().expect("");
        let stacks_node_addr = match &link.stacks_node_addr {
            Some(addr) => addr.clone(),
            None => if contract_id.starts_with("SP") {
                "https://stacks-node-api.mainnet.stacks.co".to_string()
            } else {
                "https://stacks-node-api.testnet.stacks.co".to_string()
            }
        };

        #[derive(Deserialize, Debug)]
        struct Contract {
            source: String,
            publish_height: u32,
        }

        let request_url = format!(
            "{host}/v2/contracts/source/{addr}/{name}?proof=0",
            host = stacks_node_addr,
            addr = contract_deployer,
            name = contract_name
        );

        let response: Contract = reqwest::blocking::get(&request_url)
            .expect("Unable to retrieve contract")
            .json()
            .expect("Unable to parse contract");
        let code = response.source.to_string();
        let deps = self.interpreter.detect_dependencies(contract_id.to_string(), code.clone())
            .unwrap();
        Ok((code, deps))
    }

    pub fn resolve_link(&mut self, link: &InitialLink) -> Result<Vec<(String, String, Vec<String>)>, String> {
        let mut resolved_link = Vec::new();

        let mut handled: HashMap<String, String> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_front(link.clone());
        
        while let Some(initial_link) = queue.pop_front() {
            let contract_id = &initial_link.contract_id;
            let components: Vec<&str> = contract_id.split('.').collect();
            let contract_deployer = components.first().expect("");
            let contract_name = components.last().expect("");
    
            // Extract principal from contract_id
            let (contract_code, deps) = match handled.get(contract_id) {
                Some(entry) => (entry.clone(), BTreeSet::new()),
                None => {
                    let (contract_code, deps) = self.retrieve_contract(&initial_link)
                        .expect("Unable to get contract");
                    handled.insert(contract_id.to_string(), contract_code.clone());
                    (contract_code, deps)
                }
            };

            if deps.len() > 0 {
                dependencies.insert(contract_id.to_string(), deps.clone().into_iter().collect());
                for contract_id in deps.into_iter() {
                    queue.push_back(InitialLink {
                        contract_id,
                        cache: initial_link.cache.clone(),
                        stacks_node_addr: initial_link.stacks_node_addr.clone(),
                    });
                }
                queue.push_back(initial_link);
            } else {
                let deps = match dependencies.get(contract_id) {
                    Some(deps) => deps.clone(),
                    None => vec![],
                };
                resolved_link.push((contract_id.to_string(), contract_code, deps));
            }
        }

        Ok(resolved_link)
    }

    pub fn start(&mut self) -> String {
        let mut output = Vec::<String>::new();

        if self.settings.initial_links.len() > 0 {
            let initial_links = self.settings.initial_links.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();

            let mut all_contracts = Vec::new();
            let mut indexed = BTreeSet::new();
            for link in initial_links.iter() {
                let contracts = self.resolve_link(link).unwrap();
                for (contract_id, code, _) in contracts.into_iter() {
                    if !indexed.contains(&contract_id) {
                        indexed.insert(contract_id.clone());
                        all_contracts.push((contract_id, code));
                    }
                }
            }
            for (contract_id, code) in all_contracts.into_iter() {
                let components: Vec<&str> = contract_id.split('.').collect();
                let contract_deployer = components.first().expect("");
                let contract_name = components.last().expect("");

                let deployer = {
                    PrincipalData::parse_standard_principal(&contract_deployer)
                        .expect("Unable to parse deployer's address")
                };

                self.interpreter.set_tx_sender(deployer);
                match self.formatted_interpretation(code.to_string(), Some(contract_name.to_string())) {
                    Ok(_) => {},
                    Err(ref mut result) => output.append(result),
                };
            }

            self.interpreter.set_tx_sender(default_tx_sender);
            output.push(blue!("Initial links"));
        }

        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                let recipient = match PrincipalData::parse(&account.address) {
                    Ok(recipient) => recipient,
                    _ => {
                        output.push(red!("Unable to parse address to credit"));
                        continue;
                    }
                };

                match self
                    .interpreter
                    .credit_stx_balance(recipient, account.balance)
                {
                    Ok(_) => {},
                    Err(err) => output.push(red!(err)),
                };
            }
        }

        if self.settings.include_boot_contracts {
            let default_tx_sender = self.interpreter.get_tx_sender();

            let boot_testnet_address = "ST000000000000000000002AMW42H";
            let boot_testnet_deployer = PrincipalData::parse_standard_principal(&boot_testnet_address)
                .expect("Unable to parse deployer's address");            
            self.interpreter.set_tx_sender(boot_testnet_deployer);
            self.formatted_interpretation(POX_CONTRACT.to_string(), Some("pox".to_string()))
                .expect("Unable to deploy POX");
            self.formatted_interpretation(BNS_CONTRACT.to_string(), Some("bns".to_string()))
                .expect("Unable to deploy BNS");
            self.formatted_interpretation(COSTS_CONTRACT.to_string(), Some("costs".to_string()))
                .expect("Unable to deploy COSTS");
            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer = PrincipalData::parse_standard_principal(&boot_mainnet_address)
                .expect("Unable to parse deployer's address");            
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
            self.formatted_interpretation(POX_CONTRACT.to_string(), Some("pox".to_string()))
                .expect("Unable to deploy POX");
            self.formatted_interpretation(BNS_CONTRACT.to_string(), Some("bns".to_string()))
                .expect("Unable to deploy BNS");
            self.formatted_interpretation(COSTS_CONTRACT.to_string(), Some("costs".to_string()))
                .expect("Unable to deploy COSTS");
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        if self.settings.initial_contracts.len() > 0 {
            let mut initial_contracts = self.settings.initial_contracts.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();
            for contract in initial_contracts.drain(..) {
                let deployer = {
                    let address = match contract.deployer {
                        Some(ref entry) => entry.clone(),
                        None => format!("{}", StacksAddress::burn_address(false))
                    };
                    PrincipalData::parse_standard_principal(&address)
                        .expect("Unable to parse deployer's address")
                };

                self.interpreter.set_tx_sender(deployer);
                match self.formatted_interpretation(contract.code, contract.name) {
                    Ok(_) => {},
                    Err(ref mut result) => output.append(result),
                };
            }
            self.interpreter.set_tx_sender(default_tx_sender);
            output.push(blue!("Initialized contracts"));
            self.get_contracts(&mut output);
        }

        if self.settings.initial_accounts.len() > 0 {
            output.push(blue!("Initialized balances"));
            self.get_accounts(&mut output);
        }

        output.join("\n")
    }

    pub fn check(&mut self) -> Result<(), String> {
        let mut output = Vec::<String>::new();

        if self.settings.include_boot_contracts {
            let default_tx_sender = self.interpreter.get_tx_sender();

            let boot_testnet_address = "ST000000000000000000002AMW42H";
            let boot_testnet_deployer = PrincipalData::parse_standard_principal(&boot_testnet_address)
                .expect("Unable to parse deployer's address");            
            self.interpreter.set_tx_sender(boot_testnet_deployer);
            self.formatted_interpretation(POX_CONTRACT.to_string(), Some("pox".to_string()))
                .expect("Unable to deploy POX");
            self.formatted_interpretation(BNS_CONTRACT.to_string(), Some("bns".to_string()))
                .expect("Unable to deploy BNS");
            self.formatted_interpretation(COSTS_CONTRACT.to_string(), Some("costs".to_string()))
                .expect("Unable to deploy COSTS");
            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer = PrincipalData::parse_standard_principal(&boot_mainnet_address)
                .expect("Unable to parse deployer's address");            
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
            self.formatted_interpretation(POX_CONTRACT.to_string(), Some("pox".to_string()))
                .expect("Unable to deploy POX");
            self.formatted_interpretation(BNS_CONTRACT.to_string(), Some("bns".to_string()))
                .expect("Unable to deploy BNS");
            self.formatted_interpretation(COSTS_CONTRACT.to_string(), Some("costs".to_string()))
                .expect("Unable to deploy COSTS");
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                let recipient = match PrincipalData::parse(&account.address) {
                    Ok(recipient) => recipient,
                    _ => {
                        output.push(red!("Unable to parse address to credit"));
                        continue;
                    }
                };

                match self
                    .interpreter
                    .credit_stx_balance(recipient, account.balance)
                {
                    Ok(_) => {},
                    Err(err) => output.push(red!(err)),
                };
            }
        }

        if self.settings.initial_contracts.len() > 0 {
            let mut initial_contracts = self.settings.initial_contracts.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();
            for contract in initial_contracts.drain(..) {
                let deployer = {
                    let address = match contract.deployer {
                        Some(ref entry) => entry.clone(),
                        None => format!("{}", StacksAddress::burn_address(false))
                    };
                    PrincipalData::parse_standard_principal(&address)
                        .expect("Unable to parse deployer's address")
                };

                self.interpreter.set_tx_sender(deployer);
                match self.formatted_interpretation(contract.code, contract.name) {
                    Ok(_) => {},
                    Err(ref mut result) => output.append(result),
                };
            }
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        match output.len() {
            0 => Ok(()),
            _ => Err(output.join("\n"))
        }
    }


    pub fn handle_command(&mut self, command: &str) -> Vec<String> {
        let mut output = Vec::<String>::new();
        match command {
            "::help" => self.display_help(&mut output),
            cmd if cmd.starts_with("::list_functions") => self.display_functions(&mut output),
            cmd if cmd.starts_with("::describe_function") => self.display_doc(&mut output, cmd),
            cmd if cmd.starts_with("::mint_stx") => self.mint_stx(&mut output, cmd),
            cmd if cmd.starts_with("::set_tx_sender") => self.parse_and_set_tx_sender(&mut output, cmd),
            cmd if cmd.starts_with("::get_assets_maps") => self.get_accounts(&mut output),
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
        let light_red = Colour::Red.bold();

        let result = self.interpret(snippet.to_string(), name);
        let mut output = Vec::<String>::new();

        match result {
            Ok(result) => {
                if let Some((contract_name, _)) = result.contract {
                    let snippet = format!("→ .{} contract successfully stored. Use (contract-call? ...) for invoking the public functions:", contract_name.clone());
                    output.push(green!(snippet));
                }
                if result.events.len() > 0 {
                    output.push(black!("Events emitted"));
                    for event in result.events.iter() {
                        output.push(black!(format!("{}", event)));
                    }
                }
                if let Some(result) = result.result {
                    output.push(green!(result));
                }
                Ok(output)
            }
            Err((message, diagnostic)) => {
                output.push(red!(message));
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
    ) -> Result<ExecutionResult, (String, Option<Diagnostic>)> {
        let contract_name = match name {
            Some(name) => name,
            None => format!("contract-{}", self.contracts.len()),
        };
        let first_char = contract_name.chars().next().unwrap();

        // Kludge for handling fully qualified contract_id vs sugared syntax
        let contract_identifier = if first_char.to_string() == "S" {
            QualifiedContractIdentifier::parse(&contract_name).unwrap()
        } else {
            let tx_sender = self.interpreter.get_tx_sender().to_address();
            let id = format!("{}.{}", tx_sender, contract_name);
            QualifiedContractIdentifier::parse(&id).unwrap()
        };

        match self.interpreter.run(snippet, contract_identifier.clone()) {
            Ok(result) => {
                if let Some((ref contract_identifier, ref contract)) = result.contract {
                    self.contracts.insert(contract_identifier.clone(), contract.clone());
                }
                Ok(result)
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
                .paint("::get_assets_maps\t\t\t\tGet assets maps for active accounts")
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

        if args.len() != 2 {
            output.push(red!("Usage: ::advance_chain_tip <count>"));
            return;
        }

        let count = match args[1].parse::<u32>() {
            Ok(count) => count,
            _ => {
                output.push(red!("Unable to parse count"));
                return;
            }
        };

        let new_height = self.advance_chain_tip(count);
        output.push(green!(format!("{} blocks simulated, new height: {}", count, new_height)));
    }

    pub fn advance_chain_tip(
        &mut self,
        count: u32,
    ) -> u32 {
        self.interpreter.advance_chain_tip(count)
    }

    fn parse_and_set_tx_sender(&mut self, output: &mut Vec<String>, command: &str) {
        let args: Vec<_> = command.split(' ').collect();

        if args.len() != 2 {
            output.push(red!("Usage: ::set_tx_sender <address>"));
            return;
        }

        let tx_sender = match PrincipalData::parse_standard_principal(&args[1]) {
            Ok(address) => address,
            _ => {
                output.push(red!("Unable to parse the address"));
                return;
            }
        };

        self.set_tx_sender(tx_sender.to_address());
        output.push(green!(format!("tx-sender switched to {}", tx_sender)));
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
        let height = self.interpreter.get_block_height();
        output.push(green!(format!("Current height: {}", height)));
    }
    
    fn get_account_name(&self, address: &String) -> Option<&String> {
        for account in self.settings.initial_accounts.iter() {
            if &account.address == address {
                return Some(&account.name)
            }
        }
        None
    }

    pub fn get_assets_maps(&self) -> BTreeMap<String, BTreeMap<String, u128>> {
        self.interpreter.get_assets_maps()
    }

    #[cfg(feature = "cli")]
    fn get_accounts(&mut self, output: &mut Vec<String>) {
        let accounts = self.interpreter.get_accounts();
        if accounts.len() > 0 {
            let tokens = self.interpreter.get_tokens();
            let mut headers = vec!["Address".to_string()];
            headers.append(&mut tokens.clone());
            let mut headers_cells = vec![];
            for header in headers.iter() {
                headers_cells.push(Cell::new(&header));
            }
            let mut table = Table::new();
            table.add_row(Row::new(headers_cells));
            for account in accounts.iter() {
                let mut cells = vec![];
                
                if let Some(name) = self.get_account_name(account) {
                    cells.push(Cell::new(&format!("{} ({})", account, name)));
                } else {
                    cells.push(Cell::new(account));
                }

                for token in tokens.iter() {
                    let balance = self.interpreter.get_balance_for_account(account, token);
                    cells.push(Cell::new(&format!("{}", balance)));
                }
                table.add_row(Row::new(cells));
            }
            output.push(format!("{}", table));
        }
    }

    #[cfg(feature = "cli")]
    fn get_contracts(&mut self, output:&mut Vec<String>) {
        if self.settings.initial_contracts.len() > 0 {
            let mut table = Table::new();
            table.add_row(row!["Contract identifier", "Public functions"]);
            let contracts = self.contracts.clone();
            for (contract_id, methods) in contracts.iter() {
                if !contract_id.ends_with(".pox") && !contract_id.ends_with(".bns") && !contract_id.ends_with(".costs") {
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
                    table.add_row(Row::new(vec![
                        Cell::new(&contract_id),
                        Cell::new(&formatted_spec)]));
                }
            }
            output.push(format!("{}", table));
        }
    }

    #[cfg(not(feature = "cli"))]
    fn get_accounts(&mut self, output:&mut Vec<String>) {
        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                output.push(format!("{}: {} ({})", account.address, account.balance, account.name));
            }
        }
    }

    #[cfg(not(feature = "cli"))]
    fn get_contracts(&mut self, output:&mut Vec<String>) {
        if self.settings.initial_contracts.len() > 0 {
            let mut initial_contracts = self.contracts.clone();
            for contract in initial_contracts.drain(..) {
                output.push(format!("{}", contract.0));
            }
        }
    }

    fn mint_stx(&mut self, output: &mut Vec<String>, command: &str) {
        let args: Vec<_> = command.split(' ').collect();

        if args.len() != 3 {
            output.push(red!("Usage: ::mint_stx <recipient address> <amount>"));
            return;
        }

        let recipient = match PrincipalData::parse(&args[1]) {
            Ok(address) => address,
            _ => {
                output.push(red!("Unable to parse the address"));
                return;
            }
        };

        let amount: u64 = match args[2].parse() {
            Ok(recipient) => recipient,
            _ => {
                output.push(red!("Unable to parse the balance"));
                return;
            }
        };

        match self.interpreter.credit_stx_balance(recipient, amount) {
            Ok(msg) => output.push(green!(msg)),
            Err(err) => output.push(red!(err)),
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
            s = s.replace("::describe_function", "");
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
