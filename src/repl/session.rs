use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::{fs, str::FromStr};

use ansi_term::{Colour, Style};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use tracing::{error, event, info, info_span, span, Level};
use tracing_subscriber::prelude::*;

use crate::clarity::analysis::ContractAnalysis;
use crate::clarity::ast::ContractAST;
use crate::clarity::coverage::{CoverageReporter, TestCoverageReport};
use crate::clarity::docs::{make_api_reference, make_define_reference, make_keyword_reference};
use crate::clarity::functions::define::DefineFunctions;
use crate::clarity::functions::NativeFunctions;
use crate::clarity::types::{PrincipalData, QualifiedContractIdentifier, StandardPrincipalData};
use crate::clarity::util::StacksAddress;
use crate::clarity::variables::NativeVariables;
use crate::contracts::{BNS_CONTRACT, COSTS_CONTRACT, POX_CONTRACT};
use crate::{clarity::diagnostic::Diagnostic, repl::settings::InitialContract};

use super::{ClarityInterpreter, ExecutionResult, OutputMode};

#[cfg(feature = "cli")]
use prettytable::{Cell, Row, Table};

use super::settings::InitialLink;
use super::SessionSettings;

impl Default for OutputMode {
    fn default() -> Self {
        OutputMode::Console
    }
}

impl FromStr for OutputMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(Self::Json),
            "console" => Ok(Self::Console),
            _ => Err(format!("Unknown output format {}", s)),
        }
    }
}

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

pub type Contracts = BTreeMap<String, BTreeMap<String, Vec<String>>>;

/// Get **only** user `Contracts`
pub trait GetUserContracts {
    fn get_user_contracts(&self) -> Self;
}

use std::path::Path;

impl GetUserContracts for Contracts {
    fn get_user_contracts(&self) -> Self {
        let mut user_contracts = Contracts::new();
        for (contract_id, methods) in self.iter() {
            let ext = std::path::Path::new(contract_id)
                .extension()
                .unwrap()
                .to_str()
                .unwrap();

            match ext {
                "bns" | "pox" | "costs" => continue,
                _ => {
                    user_contracts.insert(contract_id.to_owned(), methods.clone());
                }
            }
        }
        user_contracts
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    session_id: u32,
    started_at: u32,
    pub settings: SessionSettings,
    pub contracts: Contracts,
    pub asts: BTreeMap<QualifiedContractIdentifier, ContractAST>,
    pub interpreter: ClarityInterpreter,
    api_reference: HashMap<String, String>,
    pub coverage_reports: Vec<TestCoverageReport>,
    pub initial_contracts_analysis: Vec<(ContractAnalysis, String, String)>,
    pub output_mode: OutputMode,
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

        let output_mode = settings.output_mode.clone();
        Session {
            session_id: 0,
            started_at: 0,
            settings,
            asts: BTreeMap::new(),
            contracts: BTreeMap::new(),
            interpreter: ClarityInterpreter::new(tx_sender),
            api_reference: build_api_reference(),
            coverage_reports: vec![],
            initial_contracts_analysis: vec![],
            output_mode,
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
    // pub fn start(&mut self) -> Vec<(ContractAnalysis, String)> {
    pub fn start(&mut self) -> anyhow::Result<(String, Vec<(ContractAnalysis, String, String)>)> {

       let mut contracts = vec![];
        if !self.settings.include_boot_contracts.is_empty() {
            let default_tx_sender = self.interpreter.get_tx_sender();

            let boot_testnet_address = "ST000000000000000000002AMW42H";
            let boot_testnet_deployer =
                PrincipalData::parse_standard_principal(&boot_testnet_address)
                    .expect("Unable to parse deployer's address");

            self.interpreter.set_tx_sender(boot_testnet_deployer);
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
                .contains(&"costs".to_string())
            {
                self.formatted_interpretation(
                    COSTS_CONTRACT.to_string(),
                    Some("costs".to_string()),
                    false,
                    None,
                )?;
                
            }

            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer =
                PrincipalData::parse_standard_principal(&boot_mainnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
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
                .contains(&"costs".to_string())
            {
                self.formatted_interpretation(
                    COSTS_CONTRACT.to_string(),
                    Some("costs".to_string()),
                    false,
                    None,
                )
                .expect("Unable to deploy COSTS");
            }
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
                    Err(ref mut result) => error!("{:?}", result),
                };
            }

            self.interpreter.set_tx_sender(default_tx_sender);
        }

        if self.settings.initial_accounts.len() > 0 {
            let initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.into_iter() {
                let recipient = match PrincipalData::from_str(&account.address) {
                    Ok(recipient) => recipient,
                    _ => {
                        error!("Unable to parse address to credit");
                        continue;
                    }
                };

                match self
                    .interpreter
                    .credit_stx_balance(recipient, account.balance)
                {
                    Ok(_) => {}
                    Err(err) => error!("{}", err),
                };
            }
        }

        if self.settings.initial_contracts.len() > 0 {
            let initial_contracts = self.settings.initial_contracts.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();
            for contract in initial_contracts.into_iter() {
                let deployer = {
                    contract.deployer.and_then(|address| 
                    PrincipalData::parse_standard_principal(&address).ok())
                };

                self.interpreter.set_tx_sender(deployer.unwrap());
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
                    Err(err) => error!("{:?}", err),
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

        if !self.settings.initial_contracts.is_empty() {
            let user_contracts = self.contracts.get_user_contracts();
        }

        // if self.settings.initial_accounts.len() > 0 {
        //     output.push(blue!("Initialized balances"));
        //     self.interpreter.get_accounts();
        // }

        Ok(("".to_string(), contracts))
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
                .contains(&"costs".to_string())
            {
                self.formatted_interpretation(
                    COSTS_CONTRACT.to_string(),
                    Some("costs".to_string()),
                    false,
                    None,
                )
                .expect("Unable to deploy COSTS");
            }

            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer =
                PrincipalData::parse_standard_principal(&boot_mainnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
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
                .contains(&"costs".to_string())
            {
                self.formatted_interpretation(
                    COSTS_CONTRACT.to_string(),
                    Some("costs".to_string()),
                    false,
                    None,
                )
                .expect("Unable to deploy COSTS");
            }
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

    pub fn check(&mut self) -> anyhow::Result<Vec<(ContractAnalysis, String, String)>> {
        let mut error_output = Vec::<String>::new();
        let mut contracts = vec![];

        if !self.settings.include_boot_contracts.is_empty() {
            let default_tx_sender = self.interpreter.get_tx_sender();

            let boot_testnet_address = "ST000000000000000000002AMW42H";
            let boot_testnet_deployer =
                PrincipalData::parse_standard_principal(&boot_testnet_address)
                    .expect("Unable to parse deployer's address");

            self.interpreter.set_tx_sender(boot_testnet_deployer);
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
                .contains(&"costs".to_string())
            {
                self.formatted_interpretation(
                    COSTS_CONTRACT.to_string(),
                    Some("costs".to_string()),
                    false,
                    None,
                )
                .expect("Unable to deploy COSTS");
            }

            let boot_mainnet_address = "SP000000000000000000002Q6VF78";
            let boot_mainnet_deployer =
                PrincipalData::parse_standard_principal(&boot_mainnet_address)
                    .expect("Unable to parse deployer's address");
            self.interpreter.set_tx_sender(boot_mainnet_deployer);
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
                .contains(&"costs".to_string())
            {
                self.formatted_interpretation(
                    COSTS_CONTRACT.to_string(),
                    Some("costs".to_string()),
                    false,
                    None,
                )
                .expect("Unable to deploy COSTS");
            }
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        if self.settings.initial_accounts.len() > 0 {
            let mut initial_accounts = self.settings.initial_accounts.clone();
            for account in initial_accounts.drain(..) {
                let recipient = match PrincipalData::from_str(&account.address) {
                    Ok(recipient) => recipient,
                    _ => {
                        error_output.push(red!("Unable to parse address to credit"));
                        continue;
                    }
                };

                match self
                    .interpreter
                    .credit_stx_balance(recipient, account.balance)
                {
                    Ok(_) => {}
                    Err(err) => error_output.push(red!(err)),
                };
            }
        }

        if self.settings.initial_contracts.len() > 0 {
            let mut initial_contracts = self.settings.initial_contracts.clone();
            let default_tx_sender = self.interpreter.get_tx_sender();
            for contract in initial_contracts.drain(..) {
                if let Some(ref scoping_contract) = self.settings.scoping_contract {
                    if contract.path.eq(scoping_contract) {
                        break;
                    }
                }

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
                    Err(ref mut result) => error_output.push(result.to_string()),
                };
            }
            self.interpreter.set_tx_sender(default_tx_sender);
        }

        match error_output.len() {
            0 => Ok(contracts),
            _ => bail!(error_output.join("\n")),
        }
    }

    pub fn formatted_interpretation<T: AsRef<str>>(
        &mut self,
        snippet: T,
        name: Option<String>,
        cost_track: bool,
        test_name: Option<String>,
    ) -> anyhow::Result<(Vec<String>, ExecutionResult)> {
        let snippet = snippet.as_ref();
        let light_red = Colour::Red.bold();

        let result = self.interpret(snippet, name, cost_track, test_name);
        let mut output = Vec::<String>::new();

        match result {
            Ok(result) => {
                if let Some((ref contract_name, _, _, _, _)) = result.contract {
                    let snippet = format!("→ .{} contract successfully stored. Use (contract-call? ...) for invoking the public functions:", contract_name.clone());
                    output.push(snippet);
                }
                if result.events.len() > 0 {
                    output.push("Events emitted".into());
                    for event in result.events.iter() {
                        output.push(format!("{}", event));
                    }
                }
                if let Some(ref result) = result.result {
                    output.push(green!(result));
                }
                Ok((output, result))
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
                bail!("{:?}",output)
            }
        }
    }

    pub fn interpret<T: AsRef<str>>(
        &mut self,
        snippet: T,
        name: Option<String>,
        cost_track: bool,
        test_name: Option<String>,
    ) -> Result<ExecutionResult, (String, Option<Diagnostic>)> {
        let contract_name = match name {
            Some(name) => name,
            None => format!("contract-{}", self.contracts.len()),
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

    fn display_help(&self) -> Vec<String> {
        let mut output = Vec::new();
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

        output
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

        self.set_tx_sender(tx_sender.clone());
        output.push(green!(format!("tx-sender switched to {}", tx_sender)));
    }

    pub fn set_tx_sender<T: Into<StandardPrincipalData>>(&mut self, address: T) {
        self.interpreter.set_tx_sender(address.into())
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

pub fn build_api_reference() -> HashMap<String, String> {
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
