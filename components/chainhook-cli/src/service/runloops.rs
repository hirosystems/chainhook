use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc, RwLock},
};

use chainhook_sdk::{
    chainhooks::{
        bitcoin::BitcoinChainhookInstance, stacks::StacksChainhookInstance,
        types::ChainhookInstance,
    },
    observer::ObserverCommand,
    utils::Context,
};
use threadpool::ThreadPool;

use crate::{
    config::{Config, PredicatesApi},
    scan::{
        bitcoin::scan_bitcoin_chainstate_via_rpc_using_predicate, common::PredicateScanResult,
        stacks::scan_stacks_chainstate_via_rocksdb_using_predicate,
    },
    service::{connect_to_redis_with_retry, set_predicate_interrupted_status}, storage::StacksDbConnections,
};

use super::ScanningData;

pub enum StacksScanOp {
    StartScan {
        predicate_spec: StacksChainhookInstance,
        unfinished_scan_data: Option<ScanningData>,
    },
    KillScan(String),
}

pub fn start_stacks_scan_runloop(
    config: &Config,
    stacks_scan_op_rx: crossbeam_channel::Receiver<StacksScanOp>,
    observer_command_tx: Sender<ObserverCommand>,
    ctx: &Context,
) {
    let stacks_scan_pool = ThreadPool::new(config.limits.max_number_of_concurrent_stacks_scans);
    let mut kill_signals = HashMap::new();

    while let Ok(op) = stacks_scan_op_rx.recv() {
        match op {
            StacksScanOp::StartScan {
                predicate_spec,
                unfinished_scan_data,
            } => {
                let moved_ctx = ctx.clone();
                let moved_config = config.clone();
                let observer_command_tx = observer_command_tx.clone();
                let kill_signal = Arc::new(RwLock::new(false));
                kill_signals.insert(predicate_spec.uuid.clone(), kill_signal.clone());
                stacks_scan_pool.execute(move || {
                    let mut db_conns = match StacksDbConnections::open_readonly(
                        &moved_config.expected_cache_path(),
                        &moved_ctx,
                    ) {
                        Ok(db_conn) => db_conn,
                        Err(e) => {
                            // todo: if we repeatedly can't connect to the database, we should restart the
                            // service to get to a healthy state. I don't know if this has been an issue, though
                            // so we can monitor and possibly remove this todo
                            error!(
                                moved_ctx.expect_logger(),
                                "unable to open stacks db: {}",
                                e.to_string()
                            );
                            unimplemented!()
                        }
                    };

                    let op = scan_stacks_chainstate_via_rocksdb_using_predicate(
                        &predicate_spec,
                        unfinished_scan_data,
                        &mut db_conns,
                        &moved_config,
                        Some(kill_signal),
                        &moved_ctx,
                    );
                    let res = hiro_system_kit::nestable_block_on(op);
                    match res {
                        Ok(PredicateScanResult::Expired)
                        | Ok(PredicateScanResult::Deregistered) => {}
                        Ok(PredicateScanResult::ChainTipReached) => {
                            let _ = observer_command_tx.send(ObserverCommand::EnablePredicate(
                                ChainhookInstance::Stacks(predicate_spec),
                            ));
                        }
                        Err(e) => {
                            warn!(
                                moved_ctx.expect_logger(),
                                "Unable to evaluate predicate on Stacks chainstate: {e}",
                            );

                            // Update predicate status in redis
                            if let PredicatesApi::On(ref api_config) = moved_config.http_api {
                                let error = format!(
                                    "Unable to evaluate predicate on Stacks chainstate: {e}"
                                );
                                let mut predicates_db_conn = connect_to_redis_with_retry(&api_config.database_uri);
                                set_predicate_interrupted_status(
                                    error,
                                    &predicate_spec.key(),
                                    &mut predicates_db_conn,
                                    &moved_ctx,
                                );
                            }
                        }
                    }
                });
            }
            StacksScanOp::KillScan(predicate_uuid) => {
                let Some(kill_signal) = kill_signals.remove(&predicate_uuid) else {
                    continue;
                };
                let mut kill_signal_writer = kill_signal.write().unwrap();
                *kill_signal_writer = true;
            }
        }
    }
    stacks_scan_pool.join();
}

pub enum BitcoinScanOp {
    StartScan {
        predicate_spec: BitcoinChainhookInstance,
        unfinished_scan_data: Option<ScanningData>,
    },
    KillScan(String),
}

pub fn start_bitcoin_scan_runloop(
    config: &Config,
    bitcoin_scan_op_rx: crossbeam_channel::Receiver<BitcoinScanOp>,
    observer_command_tx: Sender<ObserverCommand>,
    ctx: &Context,
) {
    let bitcoin_scan_pool = ThreadPool::new(config.limits.max_number_of_concurrent_bitcoin_scans);
    let mut kill_signals = HashMap::new();

    while let Ok(op) = bitcoin_scan_op_rx.recv() {
        match op {
            BitcoinScanOp::StartScan {
                predicate_spec,
                unfinished_scan_data,
            } => {
                let moved_ctx = ctx.clone();
                let moved_config = config.clone();
                let observer_command_tx = observer_command_tx.clone();
                let kill_signal = Arc::new(RwLock::new(false));
                kill_signals.insert(predicate_spec.uuid.clone(), kill_signal.clone());

                bitcoin_scan_pool.execute(move || {
                    let op = scan_bitcoin_chainstate_via_rpc_using_predicate(
                        &predicate_spec,
                        unfinished_scan_data,
                        &moved_config,
                        Some(kill_signal),
                        &moved_ctx,
                    );

                    match hiro_system_kit::nestable_block_on(op) {
                        Ok(PredicateScanResult::Expired)
                        | Ok(PredicateScanResult::Deregistered) => {}
                        Ok(PredicateScanResult::ChainTipReached) => {
                            let _ = observer_command_tx.send(ObserverCommand::EnablePredicate(
                                ChainhookInstance::Bitcoin(predicate_spec),
                            ));
                        }
                        Err(e) => {
                            warn!(
                                moved_ctx.expect_logger(),
                                "Unable to evaluate predicate on Bitcoin chainstate: {e}",
                            );

                            // Update predicate status in redis
                            if let PredicatesApi::On(ref api_config) = moved_config.http_api {
                                let error = format!(
                                    "Unable to evaluate predicate on Bitcoin chainstate: {e}"
                                );
                                let mut predicates_db_conn = connect_to_redis_with_retry(&api_config.database_uri);
                                set_predicate_interrupted_status(
                                    error,
                                    &predicate_spec.key(),
                                    &mut predicates_db_conn,
                                    &moved_ctx,
                                )
                            }
                        }
                    }
                });
            }
            BitcoinScanOp::KillScan(predicate_uuid) => {
                let Some(kill_signal) = kill_signals.remove(&predicate_uuid) else {
                    continue;
                };
                let mut kill_signal_writer = kill_signal.write().unwrap();
                *kill_signal_writer = true;
            }
        }
    }
    bitcoin_scan_pool.join();
}
