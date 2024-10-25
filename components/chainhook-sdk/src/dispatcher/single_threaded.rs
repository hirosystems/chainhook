use std::fmt::Debug;

use reqwest::Client;
use tokio::{runtime::Builder, sync::mpsc::UnboundedReceiver, task::JoinSet};

use crate::utils::Context;

use super::{job, ChainhookOccurrence, Location, LoopCommand, LoopFeed, LoopFeedBatch, ReceiverType, SlaveWorkerFeed};

pub fn single_threaded_loop<T>(mut rx: UnboundedReceiver<LoopCommand<T>>, ctx: Context) -> ReceiverType<T>
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    let mut set = JoinSet::new();
    let client = Client::new();
    let mut response_locations = Vec::new();
    let locs = &response_locations as *const _ as usize;
    let new_ctx = &ctx as *const _ as usize;

    rt.block_on( async {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                LoopCommand::Feed(feed) => {
                    let LoopFeed { request, data, res_tx } = feed;
                    let feed = SlaveWorkerFeed { request, data, res_tx };

                    let moved_ctx = new_ctx;
                    let moved_client = client.clone();
                    let moved_locs = locs;                

                    set.spawn(job(feed, moved_client, moved_locs, moved_ctx));
                },

                LoopCommand::BatchFeed(feed_batch) => {
                    let LoopFeedBatch { requests, res_tx } = feed_batch;

                    for (request, data) in requests {
                        let feed = SlaveWorkerFeed { request, data, res_tx:res_tx.clone() };

                        let moved_ctx = new_ctx;
                        let moved_client = client.clone();
                        let moved_locs = locs;       

                        set.spawn(job(feed, moved_client, moved_locs, moved_ctx));
                    }
                },

                LoopCommand::RegisterResultLocation(loc) => {
                    let Location { 
                        synchronize, 
                        somewhere 
                    } = loc;

                    let x = response_locations.len();
                    synchronize.send(x);

                    response_locations.push(somewhere);
                },

                LoopCommand::Full => {
                    while set.join_next().await.is_some() {}
                    break;
                },

                LoopCommand::ForceFull => break,
            }
        }
    });

    ReceiverType::SingleThreaded(rx)
}