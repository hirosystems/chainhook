use crate::config::generator::generate_config;
use crate::config::Config;
use crate::scan::bitcoin::scan_bitcoin_chainstate_via_rpc_using_predicate;
use crate::scan::stacks::{
    consolidate_local_stacks_chainstate_using_csv, scan_stacks_chainstate_via_csv_using_predicate,
};
use crate::service::http_api::document_predicate_api_server;
use crate::service::Service;
use crate::storage::{
    get_last_block_height_inserted, get_stacks_block_at_block_height, is_stacks_block_present,
    open_readonly_stacks_db_conn,
};

use chainhook_sdk::chainhooks::types::{
    BitcoinChainhookFullSpecification, BitcoinChainhookNetworkSpecification, BitcoinPredicateType,
    ChainhookFullSpecification, FileHook, HookAction, OrdinalOperations,
    StacksChainhookFullSpecification, StacksChainhookNetworkSpecification, StacksPredicate,
    StacksPrintEventBasedPredicate,
};
use chainhook_sdk::types::{BitcoinNetwork, BlockIdentifier, StacksNetwork};
use chainhook_sdk::utils::Context;
use clap::{Parser, Subcommand};
use hiro_system_kit;
use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opts {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
enum Command {
    /// Generate and test predicates
    #[clap(subcommand)]
    Predicates(PredicatesCommand),
    /// Generate configuration files
    #[clap(subcommand)]
    Config(ConfigCommand),
    /// Run a service streaming blocks and evaluating registered predicates
    #[clap(subcommand)]
    Service(ServiceCommand),
    /// Stacks related subcommands  
    #[clap(subcommand)]
    Stacks(StacksCommand),
    /// Generate documentation  
    #[clap(subcommand)]
    Docs(DocsCommand),
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
#[clap(bin_name = "predicate", aliases = &["predicate"])]
enum PredicatesCommand {
    /// Generate new predicate
    #[clap(name = "new", bin_name = "new", aliases = &["generate"])]
    New(NewPredicate),
    /// Scan blocks (one-off) from specified network and apply provided predicate
    #[clap(name = "scan", bin_name = "scan")]
    Scan(ScanPredicate),
    /// Check given predicate
    #[clap(name = "check", bin_name = "check")]
    Check(CheckPredicate),
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
#[clap(bin_name = "config", aliases = &["config"])]
enum ConfigCommand {
    /// Generate new config
    #[clap(name = "new", bin_name = "new", aliases = &["generate"])]
    New(NewConfig),
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct NewConfig {
    /// Target Devnet network
    #[clap(
        long = "devnet",
        conflicts_with = "testnet",
        conflicts_with = "mainnet"
    )]
    pub devnet: bool,
    /// Target Testnet network
    #[clap(
        long = "testnet",
        conflicts_with = "devnet",
        conflicts_with = "mainnet"
    )]
    pub testnet: bool,
    /// Target Mainnet network
    #[clap(
        long = "mainnet",
        conflicts_with = "testnet",
        conflicts_with = "devnet"
    )]
    pub mainnet: bool,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct NewPredicate {
    /// Predicate's name
    pub name: String,
    /// Generate a Bitcoin predicate
    #[clap(long = "bitcoin", conflicts_with = "stacks")]
    pub bitcoin: bool,
    /// Generate a Stacks predicate
    #[clap(long = "stacks", conflicts_with = "bitcoin")]
    pub stacks: bool,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct ScanPredicate {
    /// Chainhook spec file to scan (json format)
    pub predicate_path: String,
    /// Target Testnet network
    #[clap(long = "testnet", conflicts_with = "mainnet")]
    pub testnet: bool,
    /// Target Mainnet network
    #[clap(long = "mainnet", conflicts_with = "testnet")]
    pub mainnet: bool,
    /// Load config file path
    #[clap(
        long = "config-path",
        conflicts_with = "mainnet",
        conflicts_with = "testnet"
    )]
    pub config_path: Option<String>,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct CheckPredicate {
    /// Chainhook spec file to check (json format)
    pub predicate_path: String,
    /// Target Testnet network
    #[clap(long = "testnet", conflicts_with = "mainnet")]
    pub testnet: bool,
    /// Target Mainnet network
    #[clap(long = "mainnet", conflicts_with = "testnet")]
    pub mainnet: bool,
    /// Load config file path
    #[clap(
        long = "config-path",
        conflicts_with = "mainnet",
        conflicts_with = "testnet"
    )]
    pub config_path: Option<String>,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
enum ServiceCommand {
    /// Start chainhook-cli
    #[clap(name = "start", bin_name = "start")]
    Start(StartCommand),
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct StartCommand {
    /// Target Devnet network
    #[clap(
        long = "devnet",
        conflicts_with = "testnet",
        conflicts_with = "mainnet"
    )]
    pub devnet: bool,
    /// Target Testnet network
    #[clap(
        long = "testnet",
        conflicts_with = "devnet",
        conflicts_with = "mainnet"
    )]
    pub testnet: bool,
    /// Target Mainnet network
    #[clap(
        long = "mainnet",
        conflicts_with = "testnet",
        conflicts_with = "devnet"
    )]
    pub mainnet: bool,
    /// Load config file path
    #[clap(
        long = "config-path",
        conflicts_with = "mainnet",
        conflicts_with = "testnet",
        conflicts_with = "devnet"
    )]
    pub config_path: Option<String>,
    /// Specify relative path of the chainhooks (yaml format) to evaluate
    #[clap(long = "predicate-path")]
    pub predicates_paths: Vec<String>,
    /// Start REST API for managing predicates
    #[clap(long = "start-http-api")]
    pub start_http_api: bool,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
enum StacksCommand {
    /// Db maintenance related commands
    #[clap(subcommand)]
    Db(StacksDbCommand),
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
enum StacksDbCommand {
    /// Check integrity
    #[clap(name = "check", bin_name = "check")]
    Check(CheckDbCommand),
    /// Update database using latest Stacks archive file
    #[clap(name = "update", bin_name = "update")]
    Update(UpdateDbCommand),
    /// Retrieve a block from the Stacks db
    #[clap(name = "get", bin_name = "get")]
    GetBlock(GetBlockDbCommand),
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct CheckDbCommand {
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct UpdateDbCommand {
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct GetBlockDbCommand {
    /// Block index to retrieve
    #[clap(long = "block-height")]
    pub block_height: u64,
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct InitHordDbCommand {
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
#[clap(bin_name = "docs", aliases=&["doc"])]
enum DocsCommand {
    /// Generate new documentation for the predicate registration API.
    #[clap(subcommand)]
    #[clap(name = "api")]
    Api(ApiDocsCommand),
}

#[derive(Subcommand, PartialEq, Clone, Debug)]
enum ApiDocsCommand {
    /// Generate documentation for the predicate registration API.
    #[clap(name = "new", bin_name = "new", aliases = &["generate"])]
    Generate,
}

pub fn main() {
    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };

    let opts: Opts = match Opts::try_parse() {
        Ok(opts) => opts,
        Err(e) => {
            error!(ctx.expect_logger(), "{e}");
            process::exit(1);
        }
    };

    match hiro_system_kit::nestable_block_on(handle_command(opts, ctx.clone())) {
        Err(e) => {
            error!(ctx.expect_logger(), "{e}");
            process::exit(1);
        }
        Ok(_) => {}
    }
}

async fn handle_command(opts: Opts, ctx: Context) -> Result<(), String> {
    match opts.command {
        Command::Service(subcmd) => match subcmd {
            ServiceCommand::Start(cmd) => {
                let config =
                    Config::default(cmd.devnet, cmd.testnet, cmd.mainnet, &cmd.config_path)?;

                let predicates = cmd
                    .predicates_paths
                    .iter()
                    .map(|p| load_predicate_from_path(p))
                    .collect::<Result<Vec<ChainhookFullSpecification>, _>>()?;

                info!(ctx.expect_logger(), "Starting service...",);

                let mut service = Service::new(config, ctx);
                return service.run(predicates).await;
            }
        },
        Command::Config(subcmd) => match subcmd {
            ConfigCommand::New(cmd) => {
                use std::fs::File;
                use std::io::Write;
                let config = Config::default(cmd.devnet, cmd.testnet, cmd.mainnet, &None)?;
                let config_content = generate_config(&config.network.bitcoin_network);
                let mut file_path = PathBuf::new();
                file_path.push("Chainhook.toml");
                let mut file = File::create(&file_path)
                    .map_err(|e| format!("unable to open file {}\n{}", file_path.display(), e))?;
                file.write_all(config_content.as_bytes())
                    .map_err(|e| format!("unable to write file {}\n{}", file_path.display(), e))?;
                println!("Created file Chainhook.toml");
            }
        },
        Command::Predicates(subcmd) => match subcmd {
            PredicatesCommand::New(cmd) => {
                use uuid::Uuid;

                let id = Uuid::new_v4();

                let predicate = match (cmd.stacks, cmd.bitcoin) {
                    (true, false) => {
                        let mut networks = BTreeMap::new();

                        networks.insert(StacksNetwork::Testnet, StacksChainhookNetworkSpecification {
                            start_block: Some(34239),
                            end_block: Some(50000),
                            blocks: None,
                            predicate: StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::Contains {
                                contract_identifier: "ST1SVA0SST0EDT4MFYGWGP6GNSXMMQJDVP1G8QTTC.arkadiko-freddie-v1-1".into(),
                                contains: "vault".into(),
                            }),
                            expire_after_occurrence: None,
                            capture_all_events: None,
                            decode_clarity_values: None,
                            include_contract_abi: None,
                            action:  HookAction::FileAppend(FileHook {
                                path: "arkadiko.txt".into()
                            })
                        });

                        networks.insert(StacksNetwork::Mainnet, StacksChainhookNetworkSpecification {
                            start_block: Some(34239),
                            end_block: Some(50000),
                            blocks: None,
                            predicate: StacksPredicate::PrintEvent(StacksPrintEventBasedPredicate::Contains {
                                contract_identifier: "SP2C2YFP12AJZB4MABJBAJ55XECVS7E4PMMZ89YZR.arkadiko-freddie-v1-1".into(),
                                contains: "vault".into(),
                            }),
                            expire_after_occurrence: None,
                            capture_all_events: None,
                            decode_clarity_values: None,
                            include_contract_abi: None,
                            action:  HookAction::FileAppend(FileHook {
                                path: "arkadiko.txt".into()
                            })
                        });

                        ChainhookFullSpecification::Stacks(StacksChainhookFullSpecification {
                            uuid: id.to_string(),
                            owner_uuid: None,
                            name: "Hello world".into(),
                            version: 1,
                            networks,
                        })
                    }
                    (false, true) => {
                        let mut networks = BTreeMap::new();

                        networks.insert(
                            BitcoinNetwork::Mainnet,
                            BitcoinChainhookNetworkSpecification {
                                start_block: Some(767430),
                                end_block: Some(767430),
                                blocks: None,
                                predicate: BitcoinPredicateType::OrdinalsProtocol(
                                    OrdinalOperations::InscriptionFeed,
                                ),
                                expire_after_occurrence: None,
                                action: HookAction::FileAppend(FileHook {
                                    path: "ordinals.txt".into(),
                                }),
                                include_inputs: None,
                                include_outputs: None,
                                include_proof: None,
                                include_witness: None,
                            },
                        );

                        ChainhookFullSpecification::Bitcoin(BitcoinChainhookFullSpecification {
                            uuid: id.to_string(),
                            owner_uuid: None,
                            name: "Hello world".into(),
                            version: 1,
                            networks,
                        })
                    }
                    _ => {
                        return Err("command `predicates new` should either provide the flag --stacks or --bitcoin".into());
                    }
                };

                let content = serde_json::to_string_pretty(&predicate).unwrap();
                let mut path = PathBuf::new();
                path.push(cmd.name);

                match std::fs::metadata(&path) {
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            // need to create
                            if let Some(dirp) = PathBuf::from(&path).parent() {
                                std::fs::create_dir_all(dirp).unwrap_or_else(|e| {
                                    println!("{}", e.to_string());
                                });
                            }
                            let mut f = std::fs::OpenOptions::new()
                                .write(true)
                                .create(true)
                                .truncate(true)
                                .open(&path)
                                .map_err(|e| format!("{}", e.to_string()))?;
                            use std::io::Write;
                            let _ = f.write_all(content.as_bytes());
                        } else {
                            panic!("FATAL: could not stat {}", path.display());
                        }
                    }
                    Ok(_m) => {
                        let err = format!("File {} already exists", path.display());
                        return Err(err);
                    }
                };
            }
            PredicatesCommand::Scan(cmd) => {
                let mut config =
                    Config::default(false, cmd.testnet, cmd.mainnet, &cmd.config_path)?;
                let predicate = load_predicate_from_path(&cmd.predicate_path)?;
                match predicate {
                    ChainhookFullSpecification::Bitcoin(predicate) => {
                        let predicate_spec = match predicate
                            .into_selected_network_specification(&config.network.bitcoin_network)
                        {
                            Ok(predicate) => predicate,
                            Err(e) => {
                                return Err(format!(
                                    "Specification missing for network {:?}: {e}",
                                    config.network.bitcoin_network
                                ));
                            }
                        };

                        scan_bitcoin_chainstate_via_rpc_using_predicate(
                            &predicate_spec,
                            None,
                            &config,
                            &ctx,
                        )
                        .await?;
                    }
                    ChainhookFullSpecification::Stacks(predicate) => {
                        let predicate_spec = match predicate
                            .into_selected_network_specification(&config.network.stacks_network)
                        {
                            Ok(predicate) => predicate,
                            Err(e) => {
                                return Err(format!(
                                    "Specification missing for network {:?}: {e}",
                                    config.network.bitcoin_network
                                ));
                            }
                        };
                        // TODO: if a stacks.rocksdb is present, use it.
                        // TODO: update Stacks archive file if required.
                        scan_stacks_chainstate_via_csv_using_predicate(
                            &predicate_spec,
                            &mut config,
                            &ctx,
                        )
                        .await?;
                    }
                }
            }
            PredicatesCommand::Check(cmd) => {
                let config = Config::default(false, cmd.testnet, cmd.mainnet, &cmd.config_path)?;
                let predicate: ChainhookFullSpecification =
                    load_predicate_from_path(&cmd.predicate_path)?;

                match predicate {
                    ChainhookFullSpecification::Bitcoin(predicate) => {
                        let _ = match predicate
                            .into_selected_network_specification(&config.network.bitcoin_network)
                        {
                            Ok(predicate) => predicate,
                            Err(e) => {
                                return Err(format!(
                                    "Specification missing for network {:?}: {e}",
                                    config.network.bitcoin_network
                                ));
                            }
                        };
                    }
                    ChainhookFullSpecification::Stacks(predicate) => {
                        let _ = match predicate
                            .into_selected_network_specification(&config.network.stacks_network)
                        {
                            Ok(predicate) => predicate,
                            Err(e) => {
                                return Err(format!(
                                    "Specification missing for network {:?}: {e}",
                                    config.network.bitcoin_network
                                ));
                            }
                        };
                    }
                }
                println!("✔️ Predicate {} successfully checked", cmd.predicate_path);
            }
        },
        Command::Stacks(subcmd) => match subcmd {
            StacksCommand::Db(StacksDbCommand::GetBlock(cmd)) => {
                let config = Config::default(false, false, false, &cmd.config_path)?;
                let stacks_db = open_readonly_stacks_db_conn(&config.expected_cache_path(), &ctx)
                    .expect("unable to read stacks_db");
                match get_stacks_block_at_block_height(cmd.block_height, true, 3, &stacks_db) {
                    Ok(Some(block)) => {
                        info!(ctx.expect_logger(), "{}", json!(block));
                    }
                    Ok(None) => {
                        warn!(
                            ctx.expect_logger(),
                            "Block {} not present in database", cmd.block_height
                        );
                    }
                    Err(e) => {
                        error!(ctx.expect_logger(), "{e}",);
                    }
                }
            }
            StacksCommand::Db(StacksDbCommand::Update(cmd)) => {
                let mut config = Config::default(false, false, false, &cmd.config_path)?;
                consolidate_local_stacks_chainstate_using_csv(&mut config, &ctx).await?;
            }
            StacksCommand::Db(StacksDbCommand::Check(cmd)) => {
                let config = Config::default(false, false, false, &cmd.config_path)?;
                // Delete data, if any
                {
                    let stacks_db =
                        open_readonly_stacks_db_conn(&config.expected_cache_path(), &ctx)?;
                    let mut missing_blocks = vec![];
                    let mut min = 0;
                    let mut max = 0;
                    if let Some(tip) = get_last_block_height_inserted(&stacks_db, &ctx) {
                        min = 1;
                        max = tip;
                        for index in 1..=tip {
                            let block_identifier = BlockIdentifier {
                                index,
                                hash: "".into(),
                            };
                            if !is_stacks_block_present(&block_identifier, 3, &stacks_db) {
                                missing_blocks.push(index);
                            }
                        }
                    }
                    if missing_blocks.is_empty() {
                        info!(
                            ctx.expect_logger(),
                            "Stacks db successfully checked ({min}, {max})"
                        );
                    } else {
                        warn!(
                            ctx.expect_logger(),
                            "Stacks db includes {} missing entries ({min}, {max}): {:?}",
                            missing_blocks.len(),
                            missing_blocks
                        );
                    }
                }
            }
        },
        Command::Docs(subcmd) => match subcmd {
            DocsCommand::Api(api_docs_cmd) => match api_docs_cmd {
                ApiDocsCommand::Generate => {
                    use std::fs::File;
                    use std::io::Write;
                    let spec = document_predicate_api_server()
                        .map_err(|e| format!("unable to generate API docs: {}", e))?;
                    let mut file_path = PathBuf::new();
                    file_path.push("openapi.json");
                    let mut file = File::create(&file_path).map_err(|e| {
                        format!("unable to open file {}\n{}", file_path.display(), e)
                    })?;
                    file.write_all(spec.as_bytes()).map_err(|e| {
                        format!("unable to write file {}\n{}", file_path.display(), e)
                    })?;
                    println!("Created file openapi.json");
                }
            },
        },
    }
    Ok(())
}

pub fn load_predicate_from_path(
    predicate_path: &str,
) -> Result<ChainhookFullSpecification, String> {
    let file = std::fs::File::open(&predicate_path)
        .map_err(|e| format!("unable to read file {}\n{:?}", predicate_path, e))?;
    let mut file_reader = BufReader::new(file);
    let mut file_buffer = vec![];
    file_reader
        .read_to_end(&mut file_buffer)
        .map_err(|e| format!("unable to read file {}\n{:?}", predicate_path, e))?;
    let predicate: ChainhookFullSpecification = serde_json::from_slice(&file_buffer)
        .map_err(|e| format!("unable to parse json file {}\n{:?}", predicate_path, e))?;
    Ok(predicate)
}
