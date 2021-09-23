use super::{ClarityInterpreter, ExecutionResult};
use crate::clarity::analysis::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::coverage::{CoverageReporter, TestCoverageReport};
use crate::clarity::docs::{make_api_reference, make_define_reference, make_keyword_reference};
use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::types::{PrincipalData, QualifiedContractIdentifier, StandardPrincipalData};
use crate::clarity::util::StacksAddress;
use crate::clarity::variables::NativeVariables;
use crate::clarity::errors::Error;
use crate::contracts::{BNS_CONTRACT, COSTS_V1_CONTRACT, COSTS_V2_CONTRACT, POX_CONTRACT};
use crate::repl::CostSynthesis;
use crate::{clarity::diagnostic::Diagnostic, repl::settings::InitialContract};
use ansi_term::{Colour, Style};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[cfg(feature = "cli")]
use prettytable::{Cell, Row, Table};

use super::settings::InitialLink;
use super::SessionSettings;

#[cfg(feature = "wasm")]
use reqwest_wasm as reqwest;
enum Command {
    LoadLocalContract(String),
    LoadDeployContract(String),
    UnloadContract(String),
    ExecuteSnippet(String),
    OpenSession,
    CloseSession,
}

#[derive(Clone, Debug)]
pub struct CostsReport {
    pub test_name: String,
    pub contract_id: String,
    pub method: String,
    pub args: Vec<String>,
    pub cost_result: CostSynthesis,
}

#[derive(Clone, Debug)]
pub struct Session {
    session_id: u32,
    started_at: u32,
    pub settings: SessionSettings,
    pub contracts: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub asts: BTreeMap<QualifiedContractIdentifier, ContractAST>,
    pub interpreter: ClarityInterpreter,
    api_reference: HashMap<String, String>,
    pub coverage_reports: Vec<TestCoverageReport>,
    pub costs_reports: Vec<CostsReport>,
    pub initial_contracts_analysis: Vec<(ContractAnalysis, String, String)>,
    pub show_costs: bool,
}

impl Session {
    pub fn new(settings: SessionSettings) -> Session {
        let tx_sender = {
            let address = match settings.initial_deployer {
                Some(ref entry) => entry.address.clone(),
                None => format!("{}", StacksAddress::burn_address(false)),
            };
            PrincipalData::parse_standard_principal(&address)
                .expect("Unable to parse deployer's address")
        };

        Session {
            session_id: 0,
            started_at: 0,
            interpreter: ClarityInterpreter::new(tx_sender, settings.costs_version),
            asts: BTreeMap::new(),
            contracts: BTreeMap::new(),
            api_reference: build_api_reference(),
            coverage_reports: vec![],
            costs_reports: vec![],
            initial_contracts_analysis: vec![],
            show_costs: false,
            settings,
        }
    }

    async fn retrieve_contract(
        &mut self,
        link: &InitialLink,
    ) -> Result<(String, BTreeSet<QualifiedContractIdentifier>), String> {
        let contract_id = &link.contract_id;
        let components: Vec<&str> = contract_id.split('.').collect();
        let contract_deployer = components.first().expect("");
        let contract_name = components.last().expect("");
        let stacks_node_addr = match &link.stacks_node_addr {
            Some(addr) => addr.clone(),
            None => {
                if contract_id.starts_with("SP") {
                    "https://stacks-node-api.mainnet.stacks.co".to_string()
                } else {
                    "https://stacks-node-api.testnet.stacks.co".to_string()
                }
            }
        };

        let request_url = format!(
            "{host}/v2/contracts/source/{addr}/{name}?proof=0",
            host = stacks_node_addr,
            addr = contract_deployer,
            name = contract_name
        );

        let response = fetch_contract(request_url).await;

        let code = response.source.to_string();
        let deps = self
            .interpreter
            .detect_dependencies(contract_id.to_string(), code.clone())
            .unwrap();
        Ok((code, deps))
    }

    pub async fn resolve_link(
        &mut self,
        link: &InitialLink,
    ) -> Result<Vec<(String, String, Vec<String>)>, String> {
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
                    let (contract_code, deps) = self
                        .retrieve_contract(&initial_link)
                        .await
                        .expect("Unable to get contract");
                    handled.insert(contract_id.to_string(), contract_code.clone());
                    (contract_code, deps)
                }
            };

            if deps.len() > 0 {
                dependencies.insert(
                    contract_id.to_string(),
                    deps.clone().into_iter().map(|c| format!("{}", c)).collect(),
                );
                for contract_id in deps.into_iter() {
                    queue.push_back(InitialLink {
                        contract_id: contract_id.to_string(),
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

    #[cfg(not(feature = "wasm"))]
    pub fn start(&mut self) -> Result<(String, Vec<(ContractAnalysis, String, String)>), String> {
        let mut output_err = Vec::<String>::new();
        let mut contracts = vec![];

        if !self.settings.include_boot_contracts.is_empty() {
            let default_tx_sender = self.interpreter.get_tx_sender();

            let boot_testnet_address = "ST000000000000000000002AMW42H";
            let boot_testnet_deployer =
                PrincipalData::parse_standard_principal(&boot_testnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_testnet_deployer);
            self.include_boot_contracts();

            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer =
                PrincipalData::parse_standard_principal(&boot_mainnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
            self.include_boot_contracts();
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        let mut linked_contracts = Vec::new();

        if self.settings.initial_links.len() > 0 {
            let initial_links = self.settings.initial_links.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();

            let mut indexed = BTreeSet::new();

            let rt = tokio::runtime::Runtime::new().unwrap();
            for link in initial_links.iter() {
                let contracts = rt.block_on(async { self.resolve_link(link).await.unwrap() });
                for (contract_id, code, _) in contracts.into_iter() {
                    if !indexed.contains(&contract_id) {
                        indexed.insert(contract_id.clone());
                        linked_contracts.push((contract_id, code));
                    }
                }
            }
            for (contract_id, code) in linked_contracts.iter() {
                let components: Vec<&str> = contract_id.split('.').collect();
                let contract_deployer = components.first().expect("");
                let contract_name = components.last().expect("");

                let deployer = {
                    PrincipalData::parse_standard_principal(&contract_deployer)
                        .expect("Unable to parse deployer's address")
                };

                self.interpreter.set_tx_sender(deployer);
                match self.formatted_interpretation(
                    code.to_string(),
                    Some(contract_name.to_string()),
                    true,
                    None,
                ) {
                    Ok(_) => {}
                    Err(ref mut result) => output_err.append(result),
                };
            }

            self.interpreter.set_tx_sender(default_tx_sender);
        }

        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                let recipient = match PrincipalData::parse(&account.address) {
                    Ok(recipient) => recipient,
                    _ => {
                        output_err.push(red!("Unable to parse address to credit"));
                        continue;
                    }
                };

                match self
                    .interpreter
                    .mint_stx_balance(recipient, account.balance)
                {
                    Ok(_) => {}
                    Err(err) => output_err.push(red!(err)),
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
                        None => format!("{}", StacksAddress::burn_address(false)),
                    };
                    PrincipalData::parse_standard_principal(&address)
                        .expect("Unable to parse deployer's address")
                };

                self.interpreter.set_tx_sender(deployer);
                match self.formatted_interpretation(
                    contract.code,
                    contract.name,
                    true,
                    Some("Deployment".into()),
                ) {
                    Ok((_, result)) => {
                        if result.contract.is_none() {
                            continue;
                        }
                        let analysis_result = result.contract.unwrap();
                        contracts.push((
                            analysis_result.4.clone(),
                            analysis_result.1.clone(),
                            contract.path.clone(),
                        ))
                    }
                    Err(ref mut result) => output_err.append(result),
                };
            }
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        for (contract_id, code) in linked_contracts.into_iter() {
            let components: Vec<&str> = contract_id.split('.').collect();
            let contract_deployer = components.first().expect("");
            let contract_name = components.last().expect("");

            let deployer = {
                PrincipalData::parse_standard_principal(&contract_deployer)
                    .expect("Unable to parse deployer's address")
            };
            self.settings.initial_contracts.push(InitialContract {
                code: code.to_string(),
                path: "".into(),
                name: Some(contract_id.to_string()),
                deployer: Some(deployer.to_string()),
            });
        }

        let mut output = vec![];
        if !self.settings.initial_contracts.is_empty() {
            output.push(blue!("Contracts"));
            self.get_contracts(&mut output);
        }

        if self.settings.initial_accounts.len() > 0 {
            output.push(blue!("Initialized balances"));
            self.get_accounts(&mut output);
        }

        self.initial_contracts_analysis
            .append(&mut contracts.clone());

        match output_err.len() {
            0 => Ok((output.join("\n"), contracts)),
            _ => Err(output_err.join("\n")),
        }
    }

    pub fn include_boot_contracts(&mut self) {
        if self
            .settings
            .include_boot_contracts
            .contains(&"pox".to_string())
        {
            self.formatted_interpretation(
                POX_CONTRACT.to_string(),
                Some("pox".to_string()),
                false,
                None,
            )
            .expect("Unable to deploy POX");
        }
        if self
            .settings
            .include_boot_contracts
            .contains(&"bns".to_string())
        {
            self.formatted_interpretation(
                BNS_CONTRACT.to_string(),
                Some("bns".to_string()),
                false,
                None,
            )
            .expect("Unable to deploy BNS");
        }
        if self
            .settings
            .include_boot_contracts
            .contains(&"costs-v1".to_string())
        {
            self.formatted_interpretation(
                COSTS_V1_CONTRACT.to_string(),
                Some("costs-v1".to_string()),
                false,
                None,
            )
            .expect("Unable to deploy COSTS");
        }
        if self
            .settings
            .include_boot_contracts
            .contains(&"costs-v2".to_string())
        {
            self.formatted_interpretation(
                COSTS_V2_CONTRACT.to_string(),
                Some("costs-v2".to_string()),
                false,
                None,
            )
            .expect("Unable to deploy COSTS");
        }
    }

    #[cfg(feature = "wasm")]
    pub async fn start_wasm(&mut self) -> String {
        let mut output = Vec::<String>::new();

        if !self.settings.include_boot_contracts.is_empty() {
            let default_tx_sender = self.interpreter.get_tx_sender();

            let boot_testnet_address = "ST000000000000000000002AMW42H";
            let boot_testnet_deployer =
                PrincipalData::parse_standard_principal(&boot_testnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_testnet_deployer);
            self.include_boot_contracts();

            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer =
                PrincipalData::parse_standard_principal(&boot_mainnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
            self.include_boot_contracts();

            self.interpreter.set_tx_sender(default_tx_sender);
        }

        let mut linked_contracts = Vec::new();

        if self.settings.initial_links.len() > 0 {
            let initial_links = self.settings.initial_links.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();

            let mut indexed = BTreeSet::new();
            for link in initial_links.iter() {
                let contracts = self.resolve_link(link).await.unwrap();
                for (contract_id, code, _) in contracts.into_iter() {
                    if !indexed.contains(&contract_id) {
                        indexed.insert(contract_id.clone());
                        linked_contracts.push((contract_id, code));
                    }
                }
            }
            for (contract_id, code) in linked_contracts.iter() {
                let components: Vec<&str> = contract_id.split('.').collect();
                let contract_deployer = components.first().expect("");
                let contract_name = components.last().expect("");

                let deployer = {
                    PrincipalData::parse_standard_principal(&contract_deployer)
                        .expect("Unable to parse deployer's address")
                };

                self.interpreter.set_tx_sender(deployer);
                match self.formatted_interpretation(
                    code.to_string(),
                    Some(contract_name.to_string()),
                    true,
                    None,
                ) {
                    Ok(_) => {}
                    Err(ref mut result) => output.append(result),
                };
            }

            self.interpreter.set_tx_sender(default_tx_sender);
            self.get_contracts(&mut output);
        }

        for (contract_id, code) in linked_contracts.iter() {
            let components: Vec<&str> = contract_id.split('.').collect();
            let contract_deployer = components.first().expect("");
            let contract_name = components.last().expect("");

            let deployer = {
                PrincipalData::parse_standard_principal(&contract_deployer)
                    .expect("Unable to parse deployer's address")
            };
            self.settings.initial_contracts.push(InitialContract {
                code: code.to_string(),
                path: "".into(),
                name: Some(contract_id.to_string()),
                deployer: Some(deployer.to_string()),
            });
        }

        if !self.settings.initial_contracts.is_empty() {
            output.push(blue!("Contracts"));
            self.get_contracts(&mut output);
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
            cmd if cmd.starts_with("::set_tx_sender") => {
                self.parse_and_set_tx_sender(&mut output, cmd)
            }
            cmd if cmd.starts_with("::get_assets_maps") => self.get_accounts(&mut output),
            cmd if cmd.starts_with("::get_costs") => self.get_costs(&mut output, cmd),
            cmd if cmd.starts_with("::get_contracts") => self.get_contracts(&mut output),
            cmd if cmd.starts_with("::get_block_height") => self.get_block_height(&mut output),
            cmd if cmd.starts_with("::advance_chain_tip") => {
                self.parse_and_advance_chain_tip(&mut output, cmd)
            }
            cmd if cmd.starts_with("::toggle_costs") => self.toggle_costs(&mut output),

            snippet => {
                if self.show_costs {
                    self.get_costs(&mut output, &format!("::get_costs {}", snippet))
                } else {
                    let mut result = match self.formatted_interpretation(
                        snippet.to_string(),
                        None,
                        true,
                        None,
                    ) {
                        Ok((result, _)) => result,
                        Err(result) => result,
                    };
                    output.append(&mut result);
                }
            }
        }
        output
    }

    pub fn formatted_interpretation(
        &mut self,
        snippet: String,
        name: Option<String>,
        cost_track: bool,
        test_name: Option<String>,
    ) -> Result<(Vec<String>, ExecutionResult), Vec<String>> {
        let light_red = Colour::Red.bold();

        let result = self.interpret(snippet.to_string(), name.clone(), cost_track, test_name);
        let mut output = Vec::<String>::new();

        match result {
            Ok(result) => {
                if let Some((ref contract_name, _, _, _, _)) = result.contract {
                    let snippet = format!("→ .{} contract successfully stored. Use (contract-call? ...) for invoking the public functions:", contract_name.clone());
                    output.push(green!(snippet));
                }
                if result.events.len() > 0 {
                    output.push(black!("Events emitted"));
                    for event in result.events.iter() {
                        output.push(black!(format!("{}", event)));
                    }
                }
                if let Some(ref result) = result.result {
                    output.push(green!(result));
                }
                Ok((output, result))
            }
            Err((message, diagnostic, _)) => {
                if let Some(name) = name {
                    output.push(format!("Error found in contract {}", name)); 
                }
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
                                                span.start_column.saturating_sub(1) as usize,
                                                span.end_column.saturating_sub(1) as usize,
                                            ), // One line
                                            (true, false) => (
                                                span.start_column.saturating_sub(1) as usize,
                                                line.len().saturating_sub(1),
                                            ), // Multiline, first line
                                            (false, false) => (0, line.len().saturating_sub(1)), // Multiline, in between
                                            (false, true) => {
                                                (0, span.end_column.saturating_sub(1) as usize)
                                            } // Multiline, last line
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

    pub fn invoke_contract_call(
        &mut self,
        contract: &str,
        method: &str,
        args: &Vec<String>,
        sender: &str,
        test_name: String,
    ) -> Result<ExecutionResult, (String, Option<Diagnostic>, Option<Error>)> {
        let initial_tx_sender = self.get_tx_sender();
        // Kludge for handling fully qualified contract_id vs sugared syntax
        let first_char = contract.chars().next().unwrap();
        let contract_id = if first_char.to_string() == "S" {
            contract.to_string()
        } else {
            format!("{}.{}", initial_tx_sender, contract,)
        };

        let snippet = format!(
            "(contract-call? '{} {} {})",
            contract_id,
            method,
            args.join(" ")
        );

        self.set_tx_sender(sender.into());
        let result = self.interpret(snippet, None, true, Some(test_name.clone()))?;
        if let Some(ref cost) = result.cost {
            self.costs_reports.push(CostsReport {
                test_name,
                contract_id,
                method: method.to_string(),
                args: args.to_vec(),
                cost_result: cost.clone(),
            });
        }
        self.set_tx_sender(initial_tx_sender);
        Ok(result)
    }

    pub fn interpret(
        &mut self,
        snippet: String,
        name: Option<String>,
        cost_track: bool,
        test_name: Option<String>,
    ) -> Result<ExecutionResult, (String, Option<Diagnostic>, Option<Error>)> {
        let (contract_name, is_tx) = match name {
            Some(name) => (name, false),
            None => (format!("contract-{}", self.contracts.len()), true),
        };
        let first_char = contract_name.chars().next().unwrap();

        let report = if let Some(test_name) = test_name {
            let coverage = TestCoverageReport::new(test_name.into());
            Some(coverage)
        } else {
            None
        };

        // Kludge for handling fully qualified contract_id vs sugared syntax
        let contract_identifier = if first_char.to_string() == "S" {
            QualifiedContractIdentifier::parse(&contract_name).unwrap()
        } else {
            let tx_sender = self.interpreter.get_tx_sender().to_address();
            let id = format!("{}.{}", tx_sender, contract_name);
            QualifiedContractIdentifier::parse(&id).unwrap()
        };

        match self
            .interpreter
            .run(snippet, contract_identifier.clone(), cost_track, report)
        {
            Ok(result) => {
                if let Some(ref coverage) = result.coverage {
                    self.coverage_reports.push(coverage.clone());
                }
                if let Some((
                    ref contract_identifier_str,
                    ref source,
                    ref contract,
                    ref ast,
                    ref analysis,
                )) = result.contract
                {
                    self.asts.insert(contract_identifier.clone(), ast.clone());
                    self.contracts
                        .insert(contract_identifier_str.clone(), contract.clone());
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
            help_colour.paint(
                "::list_functions\t\t\tDisplay all the native functions available in clarity"
            )
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
            help_colour.paint("::set_tx_sender <principal>\t\tSet tx-sender variable to principal")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::get_assets_maps\t\t\tGet assets maps for active accounts")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::get_costs <expr>\t\t\tDisplay the cost analysis")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::get_contracts\t\t\t\tGet contracts")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::get_block_height\t\t\tGet current block height")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::advance_chain_tip <count>\t\tSimulate mining of <count> blocks")
        ));
        output.push(format!(
            "{}",
            help_colour.paint("::toggle_costs\t\t\t\tDisplay cost analysis after every expression")
        ))
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
        output.push(green!(format!(
            "{} blocks simulated, new height: {}",
            count, new_height
        )));
    }

    pub fn advance_chain_tip(&mut self, count: u32) -> u32 {
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
        let tx_sender =
            PrincipalData::parse_standard_principal(&address).expect("Unable to parse address");
        self.interpreter.set_tx_sender(tx_sender)
    }

    pub fn get_tx_sender(&self) -> String {
        self.interpreter.get_tx_sender().to_address()
    }

    fn get_block_height(&mut self, output: &mut Vec<String>) {
        let height = self.interpreter.get_block_height();
        output.push(green!(format!("Current height: {}", height)));
    }

    fn get_account_name(&self, address: &String) -> Option<&String> {
        for account in self.settings.initial_accounts.iter() {
            if &account.address == address {
                return Some(&account.name);
            }
        }
        None
    }

    pub fn get_assets_maps(&self) -> BTreeMap<String, BTreeMap<String, u128>> {
        self.interpreter.get_assets_maps()
    }

    pub fn toggle_costs(&mut self, output: &mut Vec<String>) {
        self.show_costs = !self.show_costs;
        output.push(green!(format!("Always show costs: {}", self.show_costs)))
    }

    #[cfg(feature = "cli")]
    pub fn get_costs(&mut self, output: &mut Vec<String>, cmd: &str) {
        let snippet = cmd.to_string().split_off("::get_costs ".len());
        let (mut result, cost) = match self.formatted_interpretation(snippet, None, true, None) {
            Ok((output, result)) => (output, result.cost.clone()),
            Err(output) => (output, None),
        };

        if let Some(cost) = cost {
            let headers = vec!["".to_string(), "Consumed".to_string(), "Limit".to_string()];
            let mut headers_cells = vec![];
            for header in headers.iter() {
                headers_cells.push(Cell::new(&header));
            }
            let mut table = Table::new();
            table.add_row(Row::new(headers_cells));
            table.add_row(Row::new(vec![
                Cell::new("Runtime"),
                Cell::new(&cost.total.runtime.to_string()),
                Cell::new(&cost.limit.runtime.to_string()),
            ]));
            table.add_row(Row::new(vec![
                Cell::new("Read count"),
                Cell::new(&cost.total.read_count.to_string()),
                Cell::new(&cost.limit.read_count.to_string()),
            ]));
            table.add_row(Row::new(vec![
                Cell::new("Read length (bytes)"),
                Cell::new(&cost.total.read_length.to_string()),
                Cell::new(&cost.limit.read_length.to_string()),
            ]));
            table.add_row(Row::new(vec![
                Cell::new("Write count"),
                Cell::new(&cost.total.write_count.to_string()),
                Cell::new(&cost.limit.write_count.to_string()),
            ]));
            table.add_row(Row::new(vec![
                Cell::new("Write length (bytes)"),
                Cell::new(&cost.total.write_length.to_string()),
                Cell::new(&cost.limit.write_length.to_string()),
            ]));
            output.push(format!("{}", table));
        }
        output.append(&mut result);
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
    fn get_contracts(&mut self, output: &mut Vec<String>) {
        if self.contracts.len() > 0 {
            let mut table = Table::new();
            table.add_row(row!["Contract identifier", "Public functions"]);
            let contracts = self.contracts.clone();
            for (contract_id, methods) in contracts.iter() {
                if !contract_id.starts_with("ST000000000000000000002AMW42H")
                    && !contract_id.starts_with("SP000000000000000000002Q6VF78")
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
                    table.add_row(Row::new(vec![
                        Cell::new(&contract_id),
                        Cell::new(&formatted_spec),
                    ]));
                }
            }
            output.push(format!("{}", table));
        }
    }

    #[cfg(not(feature = "cli"))]
    pub fn get_costs(&mut self, output: &mut Vec<String>, cmd: &str) {
        let snippet = cmd.to_string().split_off("::get_costs ".len());
        let (mut result, cost) = match self.formatted_interpretation(snippet, None, true, None) {
            Ok((output, result)) => (output, result.cost.clone()),
            Err(output) => (output, None),
        };

        if let Some(cost) = cost {
            output.push(format!(
                "Execution: {:?}\nLimit: {:?}",
                cost.total, cost.limit
            ));
        }
        output.append(&mut result);
    }

    #[cfg(not(feature = "cli"))]
    fn get_accounts(&mut self, output: &mut Vec<String>) {
        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                output.push(format!(
                    "{}: {} ({})",
                    account.address, account.balance, account.name
                ));
            }
        }
    }

    #[cfg(not(feature = "cli"))]
    fn get_contracts(&mut self, output: &mut Vec<String>) {
        for (contract_id, methods) in self.contracts.iter() {
            if !contract_id.ends_with(".pox")
                && !contract_id.ends_with(".bns")
                && !contract_id.ends_with(".costs")
            {
                output.push(format!("{}", contract_id));
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

        match self.interpreter.mint_stx_balance(recipient, amount) {
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

#[derive(Deserialize, Debug, Default, Clone)]
struct Contract {
    source: String,
    publish_height: u32,
}

async fn fetch_contract(request_url: String) -> Contract {
    let response: Contract = reqwest::get(&request_url)
        .await
        .expect("Unable to retrieve contract")
        .json()
        .await
        .expect("Unable to parse contract");
    return response;
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
