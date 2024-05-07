use std::sync::mpsc::Sender;

use chainhook_sdk::{
    chainhooks::types::{
        BitcoinChainhookSpecification, ChainhookSpecification, StacksChainhookSpecification,
    },
    observer::ObserverCommand,
    utils::Context,
};
use threadpool::ThreadPool;

use crate::{
    config::{Config, PredicatesApi},
    scan::{
        bitcoin::scan_bitcoin_chainstate_via_rpc_using_predicate,
        stacks::scan_stacks_chainstate_via_rocksdb_using_predicate,
    },
    service::{open_readwrite_predicates_db_conn_or_panic, set_predicate_interrupted_status},
    storage::open_readonly_stacks_db_conn,
};

use super::ScanningData;

pub fn start_stacks_scan_runloop(
    config: &Config,
    stacks_scan_op_rx: crossbeam_channel::Receiver<(
        StacksChainhookSpecification,
        Option<ScanningData>,
    )>,
    observer_command_tx: Sender<ObserverCommand>,
    ctx: &Context,
) {
    let stacks_scan_pool = ThreadPool::new(config.limits.max_number_of_concurrent_stacks_scans);
    while let Ok((predicate_spec, unfinished_scan_data)) = stacks_scan_op_rx.recv() {
        let moved_ctx = ctx.clone();
        let moved_config = config.clone();
        let observer_command_tx = observer_command_tx.clone();
        stacks_scan_pool.execute(move || {
            let stacks_db_conn =
                match open_readonly_stacks_db_conn(&moved_config.expected_cache_path(), &moved_ctx)
                {
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
                &stacks_db_conn,
                &moved_config,
                &moved_ctx,
            );
            let res = hiro_system_kit::nestable_block_on(op);
            let (last_block_scanned, predicate_is_expired) = match res {
                Ok(last_block_scanned) => last_block_scanned,
                Err(e) => {
                    warn!(
                        moved_ctx.expect_logger(),
                        "Unable to evaluate predicate on Stacks chainstate: {e}",
                    );

                    // Update predicate status in redis
                    if let PredicatesApi::On(ref api_config) = moved_config.http_api {
                        let error =
                            format!("Unable to evaluate predicate on Stacks chainstate: {e}");
                        let mut predicates_db_conn =
                            open_readwrite_predicates_db_conn_or_panic(api_config, &moved_ctx);
                        set_predicate_interrupted_status(
                            error,
                            &predicate_spec.key(),
                            &mut predicates_db_conn,
                            &moved_ctx,
                        );
                    }

                    return;
                }
            };
            match last_block_scanned {
                Some(last_block_scanned) => {
                    info!(
                        moved_ctx.expect_logger(),
                        "Stacks chainstate scan completed up to block: {}",
                        last_block_scanned.index
                    );
                }
                None => {
                    info!(
                        moved_ctx.expect_logger(),
                        "Stacks chainstate scan completed. 0 blocks scanned."
                    );
                }
            }
            if !predicate_is_expired {
                let _ = observer_command_tx.send(ObserverCommand::EnablePredicate(
                    ChainhookSpecification::Stacks(predicate_spec),
                ));
            }
        });
    }
    let _ = stacks_scan_pool.join();
}

pub fn start_bitcoin_scan_runloop(
    config: &Config,
    bitcoin_scan_op_rx: crossbeam_channel::Receiver<(
        BitcoinChainhookSpecification,
        Option<ScanningData>,
    )>,
    observer_command_tx: Sender<ObserverCommand>,
    ctx: &Context,
) {
    let bitcoin_scan_pool = ThreadPool::new(config.limits.max_number_of_concurrent_bitcoin_scans);

    while let Ok((predicate_spec, unfinished_scan_data)) = bitcoin_scan_op_rx.recv() {
        let moved_ctx = ctx.clone();
        let moved_config = config.clone();
        let observer_command_tx = observer_command_tx.clone();
        bitcoin_scan_pool.execute(move || {
            let op = scan_bitcoin_chainstate_via_rpc_using_predicate(
                &predicate_spec,
                unfinished_scan_data,
                &moved_config,
                &moved_ctx,
            );

            let predicate_is_expired = match hiro_system_kit::nestable_block_on(op) {
                Ok(predicate_is_expired) => predicate_is_expired,
                Err(e) => {
                    warn!(
                        moved_ctx.expect_logger(),
                        "Unable to evaluate predicate on Bitcoin chainstate: {e}",
                    );

                    // Update predicate status in redis
                    if let PredicatesApi::On(ref api_config) = moved_config.http_api {
                        let error =
                            format!("Unable to evaluate predicate on Bitcoin chainstate: {e}");
                        let mut predicates_db_conn =
                            open_readwrite_predicates_db_conn_or_panic(api_config, &moved_ctx);
                        set_predicate_interrupted_status(
                            error,
                            &predicate_spec.key(),
                            &mut predicates_db_conn,
                            &moved_ctx,
                        )
                    }
                    return;
                }
            };
            if !predicate_is_expired {
                let _ = observer_command_tx.send(ObserverCommand::EnablePredicate(
                    ChainhookSpecification::Bitcoin(predicate_spec),
                ));
            }
        });
    }
    let _ = bitcoin_scan_pool.join();
}
