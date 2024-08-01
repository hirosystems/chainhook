use std::{path::PathBuf, sync::mpsc::channel, thread::sleep, time::Duration};

use chainhook_sdk::{
    chainhooks::{
        bitcoin::{BitcoinChainhookInstance, BitcoinPredicateType},
        stacks::{StacksChainhookInstance, StacksPredicate},
        types::{BlockIdentifierIndexRule, HookAction},
    },
    types::{BitcoinNetwork, StacksNetwork},
    utils::Context,
};

use crate::{
    config::{Config, EventSourceConfig, PathConfig},
    scan::stacks::consolidate_local_stacks_chainstate_using_csv,
    service::{
        runloops::{
            start_bitcoin_scan_runloop, start_stacks_scan_runloop, BitcoinScanOp, StacksScanOp,
        },
        tests::helpers::{
            mock_bitcoin_rpc::mock_bitcoin_rpc, mock_service::setup_chainhook_service_ports,
        },
    },
};

use super::helpers::mock_stacks_node::{create_tmp_working_dir, write_stacks_blocks_to_tsv};

#[tokio::test]
async fn test_stacks_runloop_kill_scan() {
    let (working_dir, tsv_dir) = create_tmp_working_dir().unwrap_or_else(|e| {
        panic!("test failed with error: {e}");
    });

    write_stacks_blocks_to_tsv(1000, &tsv_dir).unwrap_or_else(|e| {
        std::fs::remove_dir_all(&working_dir).unwrap();
        panic!("test failed with error: {e}");
    });

    let mut config = Config::devnet_default();
    config.storage.working_dir = working_dir.clone();
    config.event_sources = vec![EventSourceConfig::StacksTsvPath(PathConfig {
        file_path: PathBuf::from(tsv_dir),
    })];

    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };

    consolidate_local_stacks_chainstate_using_csv(&mut config, &ctx)
        .await
        .unwrap_or_else(|e| {
            std::fs::remove_dir_all(&working_dir).unwrap();
            panic!("test failed with error: {e}");
        });

    let (scan_op_tx, scan_op_rx) = crossbeam_channel::unbounded();
    let (observer_command_tx, _observer_command_rx) = channel();

    let _ = hiro_system_kit::thread_named("Stacks scan runloop")
        .spawn(move || {
            start_stacks_scan_runloop(&config, scan_op_rx, observer_command_tx.clone(), &ctx);
        })
        .expect("unable to spawn thread");

    let uuid = "test".to_string();
    let predicate_spec = StacksChainhookInstance {
        uuid: uuid.clone(),
        owner_uuid: None,
        name: "idc".to_string(),
        network: StacksNetwork::Devnet,
        version: 0,
        blocks: None,
        start_block: Some(1),
        end_block: Some(1_000),
        expire_after_occurrence: None,
        capture_all_events: None,
        decode_clarity_values: None,
        include_contract_abi: None,
        predicate: StacksPredicate::BlockHeight(BlockIdentifierIndexRule::LowerThan(0)),
        action: HookAction::Noop,
        enabled: false,
        expired_at: None,
    };
    let op = StacksScanOp::StartScan {
        predicate_spec,
        unfinished_scan_data: None,
    };
    let _ = scan_op_tx.send(op);
    sleep(Duration::new(0, 500_000));
    let _ = scan_op_tx.send(StacksScanOp::KillScan(uuid));
    sleep(Duration::new(0, 500_000));
    // todo: currently the scanning runloop is a bit of a black box. we have no insight
    // into what or how many predicates are being scanned. so for this test, there's no
    // good way to determine if we successfully killed the scan.
    // this [issue](https://github.com/hirosystems/chainhook/issues/509) will give us
    // more data on these threads. When this is done we should update these tests
    // to do some actual verification that the predicate is no longer being scanned
    std::fs::remove_dir_all(&working_dir).unwrap();
}

#[tokio::test]
async fn test_stacks_bitcoin_kill_scan() {
    let (_, _, _, _, bitcoin_rpc_port, _) =
        setup_chainhook_service_ports().unwrap_or_else(|e| panic!("test failed with error: {e}"));

    let _ = hiro_system_kit::thread_named("Bitcoin rpc service")
        .spawn(move || {
            let future = mock_bitcoin_rpc(bitcoin_rpc_port, 1_000);
            hiro_system_kit::nestable_block_on(future);
        })
        .expect("unable to spawn thread");

    sleep(Duration::new(1, 0));
    let mut config = Config::devnet_default();
    config.network.bitcoind_rpc_url = format!("http://0.0.0.0:{bitcoin_rpc_port}");

    let logger = hiro_system_kit::log::setup_logger();
    let _guard = hiro_system_kit::log::setup_global_logger(logger.clone());
    let ctx = Context {
        logger: Some(logger),
        tracer: false,
    };

    let (scan_op_tx, scan_op_rx) = crossbeam_channel::unbounded();
    let (observer_command_tx, _observer_command_rx) = channel();

    let _ = hiro_system_kit::thread_named("Stacks scan runloop")
        .spawn(move || {
            start_bitcoin_scan_runloop(&config, scan_op_rx, observer_command_tx.clone(), &ctx);
        })
        .expect("unable to spawn thread");

    let uuid = "test".to_string();
    let predicate_spec = BitcoinChainhookInstance {
        uuid: uuid.clone(),
        owner_uuid: None,
        name: "idc".to_string(),
        network: BitcoinNetwork::Regtest,
        version: 0,
        blocks: None,
        start_block: Some(1),
        end_block: Some(1_000),
        expire_after_occurrence: None,
        predicate: BitcoinPredicateType::Block,
        action: HookAction::Noop,
        enabled: false,
        expired_at: None,
        include_proof: false,
        include_inputs: false,
        include_outputs: false,
        include_witness: false,
    };

    let op = BitcoinScanOp::StartScan {
        predicate_spec,
        unfinished_scan_data: None,
    };
    let _ = scan_op_tx.send(op);
    sleep(Duration::new(0, 50_000_000));
    let _ = scan_op_tx.send(BitcoinScanOp::KillScan(uuid));
    // todo: currently the scanning runloop is a bit of a black box. we have no insight
    // into what or how many predicates are being scanned. so for this test, there's no
    // good way to determine if we successfully killed the scan.
    // this [issue](https://github.com/hirosystems/chainhook/issues/509) will give us
    // more data on these threads. When this is done we should update these tests
    // to do some actual verification that the predicate is no longer being scanned
}
