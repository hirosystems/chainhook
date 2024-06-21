use crate::config::generator::generate_config;
use crate::config::Config;
use crate::scan::bitcoin::scan_bitcoin_chainstate_via_rpc_using_predicate;
use crate::scan::stacks::{
    consolidate_local_stacks_chainstate_using_csv, scan_stacks_chainstate_via_csv_using_predicate,
    scan_stacks_chainstate_via_rocksdb_using_predicate,
};
use crate::service::http_api::document_predicate_api_server;
use crate::service::Service;
use crate::storage::{
    delete_confirmed_entry_from_stacks_blocks, delete_unconfirmed_entry_from_stacks_blocks,
    get_last_block_height_inserted, get_last_unconfirmed_block_height_inserted,
    get_stacks_block_at_block_height, insert_unconfirmed_entry_in_stacks_blocks,
    is_stacks_block_present, open_readonly_stacks_db_conn, open_readwrite_stacks_db_conn,
    set_last_confirmed_insert_key,
};
use chainhook_sdk::chainhooks::bitcoin::BitcoinChainhookSpecification;
use chainhook_sdk::chainhooks::bitcoin::BitcoinChainhookSpecificationNetworkMap;
use chainhook_sdk::chainhooks::bitcoin::BitcoinPredicateType;
use chainhook_sdk::chainhooks::bitcoin::InscriptionFeedData;
use chainhook_sdk::chainhooks::bitcoin::OrdinalOperations;
use chainhook_sdk::chainhooks::stacks::StacksChainhookSpecification;
use chainhook_sdk::chainhooks::stacks::StacksChainhookSpecificationNetworkMap;
use chainhook_sdk::chainhooks::stacks::StacksPredicate;
use chainhook_sdk::chainhooks::stacks::StacksPrintEventBasedPredicate;
use chainhook_sdk::chainhooks::types::{ChainhookSpecificationNetworkMap, FileHook, HookAction};
use chainhook_sdk::types::{BitcoinNetwork, BlockIdentifier, StacksNetwork};
use chainhook_sdk::utils::{BlockHeights, Context};
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
    /// If provided, serves Prometheus metrics at localhost:{port}/metrics. If not specified, does not start Prometheus server.
    #[clap(long = "prometheus-port")]
    pub prometheus_monitoring_port: Option<u16>,
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
    /// Deletes a block from the confirmed block db and moves it to the unconfirmed block db.
    #[clap(name = "unconfirm", bin_name = "unconfirm")]
    UnconfirmBlock(UnconfirmBlockDbCommand),
    /// Get latest blocks from the unconfirmed and confirmed block db.
    #[clap(name = "get-latest", bin_name = "get-latest")]
    GetLatest(GetLatestBlocksDbCommand),
    /// Update blocks from database
    #[clap(name = "drop", bin_name = "drop")]
    Drop(DropBlockCommand),
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct GetLatestBlocksDbCommand {
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
    /// The number of blocks from the chain tip to fetch.
    #[clap(long = "count")]
    pub count: u64,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct DropBlockCommand {
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
    /// Interval of blocks (--interval 767430:800000)
    #[clap(long = "interval", conflicts_with = "blocks")]
    pub blocks_interval: Option<String>,
    /// List of blocks (--blocks 767430,767431,767433,800000)
    #[clap(long = "blocks", conflicts_with = "interval")]
    pub blocks: Option<String>,
}

#[derive(Parser, PartialEq, Clone, Debug)]
struct UnconfirmBlockDbCommand {
    /// Load config file path
    #[clap(long = "config-path")]
    pub config_path: Option<String>,
    /// The block height to unconfirm
    #[clap(long = "block-height")]
    pub block_height: u64,
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
            println!("{}", e);
            process::exit(1);
        }
    };

    match hiro_system_kit::nestable_block_on(handle_command(opts, ctx.clone())) {
        Err(e) => {
            crit!(ctx.expect_logger(), "{e}");
            process::exit(1);
        }
        Ok(_) => {}
    }
}

async fn handle_command(opts: Opts, ctx: Context) -> Result<(), String> {
    match opts.command {
        Command::Service(subcmd) => match subcmd {
            ServiceCommand::Start(cmd) => {
                let mut config =
                    Config::default(cmd.devnet, cmd.testnet, cmd.mainnet, &cmd.config_path)?;

                if cmd.prometheus_monitoring_port.is_some() {
                    config.monitoring.prometheus_monitoring_port = cmd.prometheus_monitoring_port;
                }

                let predicates = cmd
                    .predicates_paths
                    .iter()
                    .map(|p| load_predicate_from_path(p))
                    .collect::<Result<Vec<ChainhookSpecificationNetworkMap>, _>>()?;

                info!(ctx.expect_logger(), "Starting service...",);

                let mut service = Service::new(config, ctx);
                return service.run(predicates, None).await;
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

                        networks.insert(StacksNetwork::Testnet, StacksChainhookSpecification {
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

                        networks.insert(StacksNetwork::Mainnet, StacksChainhookSpecification {
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

                        ChainhookSpecificationNetworkMap::Stacks(
                            StacksChainhookSpecificationNetworkMap {
                                uuid: id.to_string(),
                                owner_uuid: None,
                                name: "Hello world".into(),
                                version: 1,
                                networks,
                            },
                        )
                    }
                    (false, true) => {
                        let mut networks = BTreeMap::new();

                        networks.insert(
                            BitcoinNetwork::Mainnet,
                            BitcoinChainhookSpecification {
                                start_block: Some(767430),
                                end_block: Some(767430),
                                blocks: None,
                                predicate: BitcoinPredicateType::OrdinalsProtocol(
                                    OrdinalOperations::InscriptionFeed(InscriptionFeedData {
                                        meta_protocols: None,
                                    }),
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

                        ChainhookSpecificationNetworkMap::Bitcoin(
                            BitcoinChainhookSpecificationNetworkMap {
                                uuid: id.to_string(),
                                owner_uuid: None,
                                name: "Hello world".into(),
                                version: 1,
                                networks,
                            },
                        )
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
                    ChainhookSpecificationNetworkMap::Bitcoin(predicate) => {
                        let predicate_spec = match predicate
                            .into_specification_for_network(&config.network.bitcoin_network)
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
                    ChainhookSpecificationNetworkMap::Stacks(predicate) => {
                        let predicate_spec = match predicate
                            .into_specification_from_network(&config.network.stacks_network)
                        {
                            Ok(predicate) => predicate,
                            Err(e) => {
                                return Err(format!(
                                    "Specification missing for network {:?}: {e}",
                                    config.network.bitcoin_network
                                ));
                            }
                        };
                        match open_readonly_stacks_db_conn(&config.expected_cache_path(), &ctx) {
                            Ok(db_conn) => {
                                let _ = consolidate_local_stacks_chainstate_using_csv(
                                    &mut config,
                                    &ctx,
                                )
                                .await;
                                scan_stacks_chainstate_via_rocksdb_using_predicate(
                                    &predicate_spec,
                                    None,
                                    &db_conn,
                                    &config,
                                    &ctx,
                                )
                                .await?;
                            }
                            Err(e) => {
                                info!(
                                    ctx.expect_logger(),
                                    "Could not open db. This will greatly increase scan times. Error: {}", e
                                );
                                scan_stacks_chainstate_via_csv_using_predicate(
                                    &predicate_spec,
                                    &mut config,
                                    &ctx,
                                )
                                .await?;
                            }
                        };
                    }
                }
            }
            PredicatesCommand::Check(cmd) => {
                let config = Config::default(false, cmd.testnet, cmd.mainnet, &cmd.config_path)?;
                let predicate: ChainhookSpecificationNetworkMap =
                    load_predicate_from_path(&cmd.predicate_path)?;

                match predicate {
                    ChainhookSpecificationNetworkMap::Bitcoin(predicate) => {
                        let _ = match predicate
                            .into_specification_for_network(&config.network.bitcoin_network)
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
                    ChainhookSpecificationNetworkMap::Stacks(predicate) => {
                        let _ = match predicate
                            .into_specification_from_network(&config.network.stacks_network)
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
            StacksCommand::Db(StacksDbCommand::UnconfirmBlock(cmd)) => {
                let config = Config::default(false, false, false, &cmd.config_path)?;
                let stacks_db_rw =
                    open_readwrite_stacks_db_conn(&config.expected_cache_path(), &ctx)
                        .expect("unable to read stacks_db");

                match get_stacks_block_at_block_height(cmd.block_height, true, 3, &stacks_db_rw) {
                    Ok(Some(block)) => {
                        let mut delete_confirmed = false;
                        let mut insert_unconfirmed = false;
                        match get_stacks_block_at_block_height(
                            cmd.block_height,
                            false,
                            3,
                            &stacks_db_rw,
                        ) {
                            Ok(Some(_)) => {
                                warn!(ctx.expect_logger(), "Block {} was found in both the confirmed and unconfirmed database. Deleting from confirmed database.", cmd.block_height);
                                delete_confirmed = true;
                            }
                            Ok(None) => {
                                info!(ctx.expect_logger(), "Block {} found in confirmed database. Deleting from confirmed database and inserting into unconfirmed.", cmd.block_height);
                                delete_confirmed = true;
                                insert_unconfirmed = true;
                            }
                            Err(e) => {
                                error!(
                                    ctx.expect_logger(),
                                    "Error making request to database: {e}",
                                );
                            }
                        }
                        if delete_confirmed {
                            if let Some(last_inserted) =
                                get_last_block_height_inserted(&stacks_db_rw, &ctx)
                            {
                                if last_inserted == block.block_identifier.index {
                                    set_last_confirmed_insert_key(
                                        &block.parent_block_identifier,
                                        &stacks_db_rw,
                                        &ctx,
                                    )?;
                                }
                            }
                            delete_confirmed_entry_from_stacks_blocks(
                                &block.block_identifier,
                                &stacks_db_rw,
                                &ctx,
                            )?;
                        }
                        if insert_unconfirmed {
                            insert_unconfirmed_entry_in_stacks_blocks(&block, &stacks_db_rw, &ctx)?;
                        }
                    }
                    Ok(None) => {
                        warn!(ctx.expect_logger(), "Block {} not present in the confirmed database. No database changes were made by this command.", cmd.block_height);
                    }
                    Err(e) => {
                        error!(ctx.expect_logger(), "Error making request to database: {e}",);
                    }
                }
            }
            StacksCommand::Db(StacksDbCommand::GetLatest(cmd)) => {
                let config = Config::default(false, false, false, &cmd.config_path)?;
                let stacks_db = open_readonly_stacks_db_conn(&config.expected_cache_path(), &ctx)
                    .expect("unable to read stacks_db");

                match get_last_block_height_inserted(&stacks_db, &ctx) {
                    Some(confirmed_tip) => {
                        let min_block = confirmed_tip - cmd.count;
                        info!(
                            ctx.expect_logger(),
                            "Getting confirmed blocks {} through {}", min_block, confirmed_tip
                        );
                        let mut confirmed_blocks = vec![];
                        let mut cursor = confirmed_tip;
                        while cursor > min_block {
                            match get_stacks_block_at_block_height(cursor, true, 3, &stacks_db) {
                                Ok(Some(block)) => {
                                    confirmed_blocks.push(block.block_identifier.index);
                                    cursor -= 1;
                                }
                                Ok(None) => {
                                    warn!(
                                        ctx.expect_logger(),
                                        "Block {} not present in confirmed database", cursor
                                    );
                                    cursor -= 1;
                                }
                                Err(e) => {
                                    error!(ctx.expect_logger(), "{e}",);
                                    break;
                                }
                            }
                        }
                        info!(
                            ctx.expect_logger(),
                            "Found confirmed blocks: {:?}", confirmed_blocks
                        );
                    }
                    None => {
                        warn!(ctx.expect_logger(), "No confirmed blocks found in db");
                    }
                };

                match get_last_unconfirmed_block_height_inserted(&stacks_db, &ctx) {
                    Some(unconfirmed_tip) => {
                        let min_block = unconfirmed_tip - cmd.count;
                        info!(
                            ctx.expect_logger(),
                            "Getting unconfirmed blocks {} through {}", min_block, unconfirmed_tip
                        );
                        let mut confirmed_blocks = vec![];
                        let mut cursor = unconfirmed_tip;
                        while cursor > min_block {
                            match get_stacks_block_at_block_height(cursor, false, 3, &stacks_db) {
                                Ok(Some(block)) => {
                                    confirmed_blocks.push(block.block_identifier.index);
                                    cursor -= 1;
                                }
                                Ok(None) => {
                                    warn!(
                                        ctx.expect_logger(),
                                        "Block {} not present in unconfirmed database", cursor
                                    );
                                    cursor -= 1;
                                }
                                Err(e) => {
                                    error!(ctx.expect_logger(), "{e}",);
                                    break;
                                }
                            }
                        }
                        info!(
                            ctx.expect_logger(),
                            "Found unconfirmed blocks: {:?}", confirmed_blocks
                        );
                    }
                    None => {
                        warn!(ctx.expect_logger(), "No confirmed blocks found in db");
                    }
                };
            }
            StacksCommand::Db(StacksDbCommand::Drop(cmd)) => {
                let config = Config::default(false, false, false, &cmd.config_path)?;
                let stacks_db_rw =
                    open_readwrite_stacks_db_conn(&config.expected_cache_path(), &ctx)
                        .expect("unable to read stacks_db");

                let block_heights = parse_blocks_heights_spec(&cmd.blocks_interval, &cmd.blocks)
                    .get_sorted_entries()
                    .unwrap();
                let total_blocks = block_heights.len();
                println!("{} blocks will be deleted. Confirm? [Y/n]", total_blocks);
                let mut buffer = String::new();
                std::io::stdin().read_line(&mut buffer).unwrap();
                if buffer.starts_with('n') {
                    return Err("Deletion aborted".to_string());
                }

                for index in block_heights.into_iter() {
                    let block_identifier = BlockIdentifier {
                        index,
                        hash: "".into(),
                    };
                    let _ = delete_unconfirmed_entry_from_stacks_blocks(
                        &block_identifier,
                        &stacks_db_rw,
                        &ctx,
                    );
                }
                info!(
                    ctx.expect_logger(),
                    "Cleaning stacks_db: {} blocks dropped", total_blocks
                );
            }
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
) -> Result<ChainhookSpecificationNetworkMap, String> {
    let file = std::fs::File::open(&predicate_path)
        .map_err(|e| format!("unable to read file {}\n{:?}", predicate_path, e))?;
    let mut file_reader = BufReader::new(file);
    let mut file_buffer = vec![];
    file_reader
        .read_to_end(&mut file_buffer)
        .map_err(|e| format!("unable to read file {}\n{:?}", predicate_path, e))?;
    let predicate: ChainhookSpecificationNetworkMap = serde_json::from_slice(&file_buffer)
        .map_err(|e| format!("unable to parse json file {}\n{:?}", predicate_path, e))?;
    Ok(predicate)
}

fn parse_blocks_heights_spec(
    blocks_interval: &Option<String>,
    blocks: &Option<String>,
) -> BlockHeights {
    let blocks = match (blocks_interval, blocks) {
        (Some(interval), None) => {
            let blocks = interval.split(':').collect::<Vec<_>>();
            let start_block: u64 = blocks
                .first()
                .expect("unable to get start_block")
                .parse::<u64>()
                .expect("unable to parse start_block");
            let end_block: u64 = blocks
                .get(1)
                .expect("unable to get end_block")
                .parse::<u64>()
                .expect("unable to parse end_block");
            BlockHeights::BlockRange(start_block, end_block)
        }
        (None, Some(blocks)) => {
            let blocks = blocks
                .split(',')
                .map(|b| b.parse::<u64>().expect("unable to parse block"))
                .collect::<Vec<_>>();
            BlockHeights::Blocks(blocks)
        }
        _ => unreachable!(),
    };
    blocks
}
