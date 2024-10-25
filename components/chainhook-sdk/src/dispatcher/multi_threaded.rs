use std::{fmt::Debug, thread, time::Duration};

use crossbeam_channel::Receiver;
use reqwest::Client;
use tokio::{runtime, sync::mpsc::{unbounded_channel, UnboundedReceiver}, task::JoinSet};

use crate::utils::Context;

use super::{job, ChainhookOccurrence, Location, LoopCommand, LoopFeed, LoopFeedBatch, ReceiverType, SlaveCommand, SlaveWorkerFeed};

pub fn multi_threaded_loop<T>(
    rx: Receiver<LoopCommand<T>>, 
    pool_size: u16, 
    ctx: &Context
) -> ReceiverType<T>
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    let mut slave_workers = Vec::new();
    let mut worker_queues = Vec::new();
    let mut response_locations = Vec::new();

    for _ in 0..pool_size {
        let moved_ctx = ctx.clone();
        let (tx, rx) = unbounded_channel();
        let slave_worker = thread::spawn(move || slave_worker(rx, moved_ctx));
        slave_workers.push(slave_worker);
        worker_queues.push(tx);
    }

    let mut i = 0;
    'outer: loop {
        i = i % pool_size;
        let mut master_queue = Vec::new();
        let inbounds = rx.try_iter(); 
        for inbound in inbounds {
            match inbound {
                LoopCommand::Feed(feed) => {
                    let LoopFeed { request, res_tx, data } = feed;
                    master_queue.push(SlaveCommand::Feed(SlaveWorkerFeed{ request, res_tx, data }));
                },

                LoopCommand::BatchFeed(feed_batch) => {
                    let LoopFeedBatch { requests, res_tx } = feed_batch;
                    for (request, data) in requests {
                        master_queue.push(SlaveCommand::Feed(SlaveWorkerFeed { request, res_tx: res_tx.clone(), data }));
                    }
                },

                LoopCommand::RegisterResultLocation(loc) => {
                    let Location { 
                        synchronize, 
                        somewhere 
                    } = loc;

                    let x = response_locations.len();
                    synchronize.send(x);

                    for sender in &worker_queues {
                        sender.send(SlaveCommand::RegisterResultLocation(somewhere.clone()));
                    }

                    response_locations.push(somewhere);
                },

                LoopCommand::Full => {
                    for _ in 0..pool_size {
                        master_queue.push(SlaveCommand::Full);
                    }

                    for slave_work in master_queue {
                        worker_queues[(i % pool_size) as usize].send(slave_work);
                        i = i + 1;
                    }

                    break 'outer;
                },

                LoopCommand::ForceFull => {
                    //maybe a atomic variable? can be used to force terminate the slave threads faster.
                    for sender in &worker_queues {
                        sender.send(SlaveCommand::ForceFull);
                    }

                    break 'outer;
                },
            }
        }

        for slave_work in master_queue {
            worker_queues[(i % pool_size) as usize].send(slave_work);
            i = i + 1;
        }

        thread::sleep(Duration::from_millis(50));
    }

    for slave in slave_workers {
        slave.join();
    }

    ReceiverType::MultiThreaded(rx)
}

fn slave_worker<T>(mut rx: UnboundedReceiver<SlaveCommand<T>>, ctx: Context)
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    let rt = runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let client = Client::new();
    let new_ctx = &ctx as *const _ as usize;
    let mut result_locations = Vec::new();
    let locs = &result_locations as *const _ as usize;
    let mut set = JoinSet::new();

    rt.block_on( async {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    SlaveCommand::Feed(feed) => {
                        let moved_ctx = new_ctx;
                        let moved_client = client.clone();
                        let moved_locs = locs;                        
                        set.spawn(job(feed, moved_client, moved_locs, moved_ctx));
                    },

                    SlaveCommand::RegisterResultLocation(here) => {
                        result_locations.push(here)
                    },

                    SlaveCommand::Full => {
                        while set.join_next().await.is_some() {}
                        break;
                    },

                    SlaveCommand::ForceFull => break,
                }
            }

    });
}