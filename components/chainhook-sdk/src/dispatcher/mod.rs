mod single_threaded;
mod multi_threaded;


use std::{fmt::Debug, sync::{atomic::{AtomicBool, AtomicI8, Ordering}, Arc, Mutex}, thread::{self, JoinHandle}};

use crossbeam_channel::{unbounded, Sender};
use hiro_system_kit::slog;
use multi_threaded::multi_threaded_loop;
use reqwest::{Client, RequestBuilder};
use single_threaded::single_threaded_loop;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    chainhooks::{bitcoin::BitcoinChainhookOccurrencePayload, stacks::StacksChainhookOccurrencePayload}, 
    utils::{send_request, Context}
};

#[derive(Debug, Clone)]
pub enum ChainhookOccurrencePayload {
    Stacks(StacksChainhookOccurrencePayload),
    Bitcoin(BitcoinChainhookOccurrencePayload),
}

pub trait ChainhookOccurrence {}

impl ChainhookOccurrence for ChainhookOccurrencePayload {}

#[derive(Debug)]
struct LoopFeedBatch<T> {
    pub requests: Vec<(Box<RequestBuilder>, Box<T>)>,
    pub res_tx: ResultLocation,
}

#[derive(Debug)]
struct LoopFeed<T> {
    pub request: Box<RequestBuilder>,
    pub data: Box<T>,
    pub res_tx: ResultLocation,
}

#[derive(Debug)]
struct SlaveWorkerFeed<T> {
    pub request: Box<RequestBuilder>,
    pub data: Box<T>,
    pub res_tx: ResultLocation,
}

#[derive(Debug, Clone)]
enum ResultLocation {
    Somewhere(usize),
    None,
}

#[derive(Debug)]
enum LoopCommand<T> {
    Feed(LoopFeed<T>),
    BatchFeed(LoopFeedBatch<T>),
    RegisterResultLocation(Location<T>),
    Full,  //LazyFull.
    ForceFull,
}

#[derive(Debug)]
enum SlaveCommand<T> {
    Feed(SlaveWorkerFeed<T>),
    RegisterResultLocation(Sender<(Box<T>, Result<(), String>)>),
    Full, // lazy graceful shutdown.
    ForceFull,
}

#[derive(Debug, Clone)]
struct Location<T> {
    pub synchronize: Sender<usize>,
    pub somewhere: Sender<(Box<T>, Result<(), String>)>,
}

async fn job<T>(feed: SlaveWorkerFeed<T>, client: Client, locs: usize, ctx: usize)
where
    T: ChainhookOccurrence + Clone + Debug
{
    let ctx = unsafe {
        &*(ctx as *const Context)
    };
    let SlaveWorkerFeed { request, res_tx, data } = feed;

    let request = request.build();

    if let Err(e) = request {
        let msg =
        format!("Unable to parse url {}", e);
        ctx.try_log(|logger| slog::warn!(logger, "{}", msg));
        return
    }

    let req = request.unwrap();
    let res = send_request(RequestBuilder::from_parts(client, req), 3, 1, ctx).await;

    match res_tx {
        ResultLocation::Somewhere(here) => {
            let locs = unsafe {
                &*(locs as *const Vec<Sender<(Box<T>, Result<(), String>)>>)
            };
            locs[here].send((data, res));
        },

        ResultLocation::None => {},
    }
}

#[derive(Clone)]
pub struct Dispatcher<T> 
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    inner: DispatcherInner<T>,
}

#[derive(Debug, Clone)]
enum Flavor {
    SingleThreaded,
    MultiThreaded(u16),
}

struct DispatcherInner<T> {
    flavor: Flavor,
    is_running: Arc<AtomicI8>,
    queue: Queue<T>,
    rx: Arc<Mutex<Receiver<T>>>, // mutex is not necessary
    handle: Arc<Mutex<Option<JoinHandle<ReceiverType<T>>>>>, // beacause of atomic but just in case
    response: Option<Response<T>>,
    ctx: Context,
}

#[derive(Debug, Clone)]
enum Queue<T> {
    SingleThreaded(SingleThreadedQueue<T>),
    MultiThreaded(MultiThreadedQueue<T>),
}

#[derive(Debug)]
enum Receiver<T> {
    Available(ReceiverType<T>),
    Taken,
}

impl<T> Default for Receiver<T> 
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    fn default() -> Self {
        Self::Taken
    }
}

#[derive(Debug)]
enum ReceiverType<T> {
    SingleThreaded(UnboundedReceiver<LoopCommand<T>>),
    MultiThreaded(crossbeam_channel::Receiver<LoopCommand<T>>),
}

#[derive(Debug)]
struct Response<T> {
    rx: crossbeam_channel::Receiver<(Box<T>, Result<(), String>)>,
    token_id: usize,
}

#[derive(Debug, Clone)]
struct SingleThreadedQueue<T> {
    tx: tokio::sync::mpsc::UnboundedSender<LoopCommand<T>>,
}

#[derive(Debug, Clone)]
struct MultiThreadedQueue<T> {
    tx: crossbeam_channel::Sender<LoopCommand<T>>,
}

impl<T> Clone for DispatcherInner<T> 
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    fn clone(&self) -> Self {
        Self { 
            flavor: self.flavor.clone(), 
            is_running: Arc::clone(&self.is_running), 
            queue: self.queue.clone(),
            rx: Arc::clone(&self.rx), 
            handle: Arc::clone(&self.handle), 
            response: None, 
            ctx: self.ctx.clone(),
        }
    }
}

impl<T> Dispatcher<T>
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    pub fn new_single_threaded(ctx: &Context) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let inner = DispatcherInner { 
            flavor: Flavor::SingleThreaded, 
            is_running: Arc::new(AtomicI8::new(0)), 
            queue: Queue::SingleThreaded(SingleThreadedQueue { tx }),
            rx: Arc::new(Mutex::new(Receiver::Available(ReceiverType::SingleThreaded(rx)))),
            handle: Arc::new(Mutex::new(Option::None)),
            response: None, 
            ctx: ctx.clone(), 
        };

        Dispatcher { inner }
    }

    pub fn new_multi_threaded(num_of_threads: u16, ctx: &Context) -> Self {
        if num_of_threads < 2 {
            return Self::new_single_threaded(ctx)
        }

        let (tx, rx) = crossbeam_channel::unbounded();

        let inner = DispatcherInner {
            flavor: Flavor::MultiThreaded(num_of_threads),
            is_running: Arc::new(AtomicI8::new(0)), 
            queue: Queue::MultiThreaded(MultiThreadedQueue { tx }),
            rx: Arc::new(Mutex::new(Receiver::Available(ReceiverType::MultiThreaded(rx)))),
            handle: Arc::new(Mutex::new(Option::None)), 
            response: None, 
            ctx: ctx.clone(),
        };

        Dispatcher { inner }
    }

    pub fn start(&mut self) {
        self.inner.start();
    }

    pub fn register_result_location(&mut self) {
        self.inner.register_result_location();
    }

    pub fn send(&self, request: RequestBuilder, data: T) {
        self.inner.send(request, data);
    }

    pub fn send_batch(&self, requests: Vec<(Box<RequestBuilder>, Box<T>)>) {
        self.inner.send_batch(requests);
    }

    pub fn recv(&self) -> Result<(Box<T>, Result<(), String>), ()> {
        self.inner.recv()
    }

    pub fn try_recv(&self) -> Result<(Box<T>, Result<(), String>), ()> {
        self.inner.try_recv()
    }

    pub fn try_iter(&self) -> Result<Vec<(Box<T>, Result<(), String>)>, ()> {
        self.inner.try_iter()
    }

    pub fn graceful_shutdown(&mut self) {
        self.inner.graceful_shutdown();
    }

    pub fn force_shutdown(&mut self) {
        self.inner.force_shutdown();
    }
}

impl<T> DispatcherInner<T>
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    fn start(&mut self) {
        match self.is_running.compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed) {
            Err(x) => {
                match x {
                    1 => {
                        let msg = format!("unable to satrt the dispatcher. An instance associated with this object is already running.");
                        return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
                    },

                    -1 => {
                        let msg = format!("try to start again.");
                        return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
                    },

                    _ => {}
                }
            },

            Ok(_x) => {},
        }

        let moved_ctx = self.ctx.clone();

        match &self.flavor {
            Flavor::SingleThreaded => {
                let rx = std::mem::take(&mut *self.rx.lock().unwrap());

                if let Receiver::Available(ReceiverType::SingleThreaded(rx)) = rx {
                    let handle = thread::spawn(|| single_threaded_loop(rx, moved_ctx));
                    *self.handle.lock().unwrap() = Some(handle);   
                }
            },

            Flavor::MultiThreaded(num_threads) => {
                let pool_size = *num_threads;
                
                let rx = std::mem::take(&mut *self.rx.lock().unwrap());

                if let Receiver::Available(ReceiverType::MultiThreaded(rx)) = rx {
                    let handle = thread::spawn(move || multi_threaded_loop(rx, pool_size, &moved_ctx));
                    *self.handle.lock().unwrap() = Some(handle);
                }
            },
        }

        self.is_running.store(1, Ordering::Release);
    }

    fn register_result_location(&mut self) {
        if 1 != self.is_running.load(Ordering::Relaxed) {
            let msg = format!("No instance of the dispatcher is running.");
            return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
        }

        if let Some(_) = &self.response {
            let msg = format!("this instance is already registered.");
            return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
        }

        match &self.queue {
            Queue::SingleThreaded(SingleThreadedQueue { tx }) => {
                let (syn_tx, syn_rx) = unbounded();
                let (res_tx, res_rx) = unbounded();
                let loc = Location { synchronize: syn_tx, somewhere: res_tx};

                tx.send(LoopCommand::RegisterResultLocation(loc));

                let token_id = syn_rx.recv().unwrap();

                self.response = Some(Response { rx: res_rx, token_id });
            },

            Queue::MultiThreaded(MultiThreadedQueue { tx }) => {
                let (syn_tx, syn_rx) = unbounded();
                let (res_tx, res_rx) = unbounded();
                let loc = Location { synchronize: syn_tx, somewhere: res_tx};

                tx.send(LoopCommand::RegisterResultLocation(loc));

                let token_id = syn_rx.recv().unwrap();

                self.response = Some(Response { rx: res_rx, token_id });
            },
        }
    }

    fn send(&self, request: RequestBuilder, data: T) {
        let request = Box::new(request);
        let data = Box::new(data);

        let res_tx = match &self.response {
            Some(res) => {
                ResultLocation::Somewhere(res.token_id)
            },

            None => ResultLocation::None,
        };

        let msg = LoopCommand::Feed(LoopFeed { request, data, res_tx });

        match &self.queue {
            Queue::SingleThreaded(SingleThreadedQueue { tx }) => {
                tx.send(msg);
            },

            Queue::MultiThreaded(MultiThreadedQueue { tx }) => {
                tx.send(msg);
            },
        }
    }

    fn send_batch(&self, requests: Vec<(Box<RequestBuilder>, Box<T>)>) {
        let res_tx = match &self.response {
            Some(res) => {
                ResultLocation::Somewhere(res.token_id)
            },

            None => ResultLocation::None,
        };

        let msg = LoopCommand::BatchFeed(LoopFeedBatch { requests, res_tx });

        match &self.queue {
            Queue::SingleThreaded(SingleThreadedQueue { tx }) => {
                tx.send(msg);
            },

            Queue::MultiThreaded(MultiThreadedQueue { tx }) => {
                tx.send(msg);
            },
        }
    }

    fn recv(&self) -> Result<(Box<T>, Result<(), String>), ()> {
        if 1 != self.is_running.load(Ordering::Relaxed) {
            let msg = format!("No instance of the dispatcher is running.");
            self.ctx.try_log(|logger| slog::info!(logger, "{}", msg));
            return Err(())
        }

        match &self.response {
            Some(res) => {
                match res.rx.recv() {
                    Ok(val) => {
                        Ok(val)
                    },

                    Err(e) => {
                        self.ctx.try_log(|logger| {
                            slog::crit!(logger, "Error: broken channel {}", e.to_string())
                        });
                        Err(())
                    },
                }
            },

            None => {
                let msg = format!("This instance of the dispatcher is not yet registered to recieve responses.");
                self.ctx.try_log(|logger| slog::info!(logger, "{}", msg));
                return Err(())
            },
        }
    }

    fn try_recv(&self) -> Result<(Box<T>, Result<(), String>), ()> {
        if 1 != self.is_running.load(Ordering::Relaxed) {
            let msg = format!("No instance of the dispatcher is running.");
            self.ctx.try_log(|logger| slog::info!(logger, "{}", msg));
            return Err(())
        }

        match &self.response {
            Some(res) => {
                match res.rx.try_recv() {
                    Ok(val) => {
                        return Ok(val)
                    },

                    Err(e) => {
                        self.ctx.try_log(|logger| {
                            slog::warn!(logger, "Error: {}", e.to_string())
                        });
                        return Err(())
                    },
                }
            },

            None => {
                let msg = format!("This instance of the dispatcher is not yet registered to recieve responses.");
                self.ctx.try_log(|logger| slog::info!(logger, "{}", msg));
                return Err(())
            },
        }
    }

    fn try_iter(&self) -> Result<Vec<(Box<T>, Result<(), String>)>, ()> {
        if 1 != self.is_running.load(Ordering::Relaxed) {
            let msg = format!("No instance of the dispatcher is running.");
            self.ctx.try_log(|logger| slog::info!(logger, "{}", msg));
            return Err(())
        }

        match &self.response {
            Some(res) => {
                let iterator = res.rx.try_iter().collect::<Vec<_>>();
                return Ok(iterator)
            },

            None => {
                let msg = format!("This instance of the dispatcher is not yet registered to recieve responses.");
                self.ctx.try_log(|logger| slog::info!(logger, "{}", msg));
                return Err(())
            },
        }
    }

    fn graceful_shutdown(&mut self) {
        match self.is_running.compare_exchange(1, -1, Ordering::Acquire, Ordering::Relaxed) {
            Err(x) => {
                match x {
                    0 => {
                        let msg = format!("No instance of the dispatcher is running.");
                        return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
                    },

                    -1 => {
                        let msg = format!("try to shutdown again.");
                        return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
                    },

                    _ => {},    
                }
            },

            Ok(_x) => {},
        }

        let msg: LoopCommand<T> = LoopCommand::Full;

        match &self.queue {
            Queue::SingleThreaded(SingleThreadedQueue { tx }) => {
                tx.send(msg);
                
                let handle = self.handle.lock().unwrap().take();

                let rx = handle.unwrap().join().unwrap();

                *self.rx.lock().unwrap() = Receiver::Available(rx);
            },

            Queue::MultiThreaded(MultiThreadedQueue { tx }) => {
                tx.send(msg);

                let handle = self.handle.lock().unwrap().take();

                let rx = handle.unwrap().join().unwrap();

                *self.rx.lock().unwrap() = Receiver::Available(rx);
            },
        }

        self.is_running.store(0, Ordering::Release);
    }

    fn force_shutdown(&mut self) {
        match self.is_running.compare_exchange(1, -1, Ordering::Acquire, Ordering::Relaxed) {
            Err(x) => {
                match x {
                    0 => {
                        let msg = format!("No instance of the dispatcher is running.");
                        return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
                    },

                    -1 => {
                        let msg = format!("try to shutdown again.");
                        return self.ctx.try_log(|logger| slog::info!(logger, "{}", msg))
                    }

                    _ => {},
                }
            },

            Ok(_x) => {},
        }

        let msg: LoopCommand<T> = LoopCommand::ForceFull;

        match &self.queue {
            Queue::SingleThreaded(SingleThreadedQueue { tx }) => {
                tx.send(msg);
                
                let handle = self.handle.lock().unwrap().take();

                let rx = handle.unwrap().join().unwrap();

                *self.rx.lock().unwrap() = Receiver::Available(rx);
            },

            Queue::MultiThreaded(MultiThreadedQueue { tx }) => {
                tx.send(msg);

                let handle = self.handle.lock().unwrap().take();

                let rx = handle.unwrap().join().unwrap();

                *self.rx.lock().unwrap() = Receiver::Available(rx);
            },
        }

        self.is_running.store(0, Ordering::Release);
    }
}

impl<T> Drop for Dispatcher<T>
where
    T: ChainhookOccurrence + Clone + Debug + Sync + Send + 'static
{
    fn drop(&mut self) {
        if 1 == Arc::strong_count(&self.inner.is_running) {
            if 1 != self.inner.is_running.load(Ordering::Relaxed) {
                return
            }

            self.force_shutdown();
        }
    }
}