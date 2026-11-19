use crate::*;
use redb::Database;
use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{oneshot, watch};
#[derive(Clone, Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Configuration {
    pub max_frame: u32,
    pub command_capacity: u32,
    pub trace_capacity: u32,
    pub max_subscriptions: u32,
    pub lock_socket: String,
    pub translator_socket: String,
    pub observer_socket: String,
    pub pause_at: Option<Boundary>,
}
pub(crate) type Job<D> = Box<dyn FnOnce(&mut Owner<D>) + Send>;
pub(crate) struct Owner<D: Domain> {
    pub(crate) stopped: bool,
    pub database: Database,
    pub subscribers: HashMap<Vec<u8>, watch::Sender<Option<Vec<u8>>>>,
    pub fail_commit: Arc<AtomicBool>,
    pub configuration: Arc<Mutex<Configuration>>,
    pub traces: TraceHub<D>,
}
#[derive(Clone)]
pub struct Scope<D: Domain> {
    pub(crate) sender: std::sync::mpsc::SyncSender<Job<D>>,
    pub configuration: Arc<Mutex<Configuration>>,
    pub traces: TraceHub<D>,
    pub context: Option<TraceContext<D>>,
    pub fail_commit: Arc<AtomicBool>,
    pub dead: watch::Receiver<bool>,
    pub(crate) held_owner: Option<u64>,
    pub(crate) holding_sequence: Arc<std::sync::atomic::AtomicU64>,
    pub(crate) output_sink: Option<OutputSink>,
    pub(crate) exchange_end: Option<watch::Receiver<bool>>,
    pub(crate) output_emitted: Arc<AtomicBool>,
}
pub trait ScopeOpening<D: Domain>: Sized {
    fn open(
        path: PathBuf,
        configuration: Configuration,
        traces: TraceHub<D>,
    ) -> impl Future<Output = Result<Self, Failure>> + Send;
}
impl<D: Domain> ScopeOpening<D> for Scope<D> {
    async fn open(
        path: PathBuf,
        configuration: Configuration,
        traces: TraceHub<D>,
    ) -> Result<Self, Failure> {
        if configuration.command_capacity == 0
            || configuration.trace_capacity == 0
            || configuration.max_frame == 0
            || configuration.max_subscriptions == 0
        {
            return Err(Failure::Limit);
        }
        let (sender, receiver) =
            std::sync::mpsc::sync_channel::<Job<D>>(configuration.command_capacity as usize);
        let (ready, wait) = oneshot::channel();
        let (dead_sender, dead) = watch::channel(false);
        let configuration = Arc::new(Mutex::new(configuration));
        let fail_commit = Arc::new(AtomicBool::new(false));
        let result = Self {
            sender,
            configuration: configuration.clone(),
            traces: traces.clone(),
            context: None,
            fail_commit: fail_commit.clone(),
            dead,
            held_owner: None,
            holding_sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            output_sink: None,
            exchange_end: None,
            output_emitted: Arc::new(AtomicBool::new(false)),
        };
        std::thread::spawn(move || {
            // A panic drops the liveness sender; serving treats that as fatal too.
            let opened = (|| {
                let database = Database::create(path).map_err(|_| Failure::Unstored)?;
                let mut tx = Transaction::<D>::begin(&database, traces.root())?;
                if let Err(failure) = tx.schema() {
                    return Err(tx.abort(failure));
                }
                tx.end_holding(None)?;
                tx.commit()?;
                Ok::<_, Failure>(Owner {
                    stopped: false,
                    database,
                    subscribers: HashMap::new(),
                    fail_commit,
                    configuration,
                    traces,
                })
            })();
            match opened {
                Err(e) => {
                    let _ = ready.send(Err(e));
                }
                Ok(mut owner) => {
                    if ready.send(Ok(())).is_ok() {
                        while let Ok(job) = receiver.recv() {
                            job(&mut owner);
                            if owner.stopped {
                                break;
                            }
                        }
                    }
                }
            }
            let _ = dead_sender.send(true);
        });
        wait.await.map_err(|_| Failure::Stopped)??;
        Ok(result)
    }
}
pub trait ChangeExecuting<D: Domain> {
    fn change<C: Changeable<D>>(
        &self,
        change: C,
    ) -> impl Future<Output = Result<C::Yield, D::Refusal>> + Send;
}
impl<D: Domain> ChangeExecuting<D> for Scope<D>
where
    D::Refusal: TraceOutcome<D>,
{
    async fn change<C: Changeable<D>>(&self, change: C) -> Result<C::Yield, D::Refusal> {
        let (reply, wait) = oneshot::channel();
        let context = self.context.clone().unwrap_or_else(|| self.traces.root());
        let held_owner = self.held_owner;
        self.sender
            .send(Box::new(move |owner| {
                // Root yield belongs to the commit, not the body's earlier success.
                let mut guard = context.enter(change.object());
                let result = (|| {
                    let mut tx = Transaction::<D>::begin(&owner.database, guard.context.clone())
                        .map_err(D::Refusal::from)?;
                    tx.held_owner = held_owner;
                    let result = change.change(&mut tx);
                    let value = match tx.refusal {
                        Some(ref r) => return Err(r.clone()),
                        None => result?,
                    };
                    if owner.fail_commit.swap(false, Ordering::SeqCst) {
                        return Err(Failure::Unstored.into());
                    }
                    let changed = tx.changed;
                    tx.write
                        .commit()
                        .map_err(|_| D::Refusal::from(Failure::Unstored))?;
                    for (key, bytes) in changed {
                        if let Some(sender) = owner.subscribers.get(&key) {
                            sender.send_replace(bytes);
                        }
                    }
                    Ok(value)
                })();
                guard.yielded(result.outcome());
                let _ = reply.send(result);
            }))
            .map_err(|_| D::Refusal::from(Failure::Stopped))?;
        wait.await.map_err(|_| D::Refusal::from(Failure::Stopped))?
    }
}
pub trait AttemptExecuting<D: Domain> {
    fn attempt<A: Operable<D>>(&self, attempt: A) -> impl Future<Output = A::Outcome> + Send;
}
impl<D: Domain> AttemptExecuting<D> for Scope<D> {
    async fn attempt<A: Operable<D>>(&self, attempt: A) -> A::Outcome {
        let mut guard = self
            .context
            .clone()
            .unwrap_or_else(|| self.traces.root())
            .enter(attempt.object());
        let child = Self {
            context: Some(guard.context.clone()),
            ..self.clone()
        };
        let mut outcome = attempt.operate(&child).await;
        guard.yielded(outcome.outcome());
        outcome.context(guard.context.clone());
        outcome
    }
}
impl<D: Domain> Runnable<D> for Scope<D> {
    async fn run<O: ScopeObject<D>>(&self, object: O) -> O::Outcome {
        object.execute(self).await
    }
}
pub trait Subscribing<D: Domain> {
    fn subscribe<R: Memorizable>(
        &self,
        identity: Vec<u8>,
    ) -> impl Future<Output = Result<Subscription<R>, Failure>> + Send;
}
impl<D: Domain> Subscribing<D> for Scope<D> {
    async fn subscribe<R: Memorizable>(
        &self,
        identity: Vec<u8>,
    ) -> Result<Subscription<R>, Failure> {
        let (reply, wait) = oneshot::channel();
        self.sender
            .send(Box::new(move |owner| {
                let result = (|| {
                    owner
                        .subscribers
                        .retain(|_, sender| sender.receiver_count() > 0);
                    let key = R::key(&identity);
                    if let Some(sender) = owner.subscribers.get(&key) {
                        return Ok(Subscription {
                            receiver: sender.subscribe(),
                            marker: std::marker::PhantomData,
                            lifecycle: None,
                        });
                    }
                    if owner.subscribers.len()
                        >= owner.configuration.lock().unwrap().max_subscriptions as usize
                    {
                        return Err(Failure::Limit);
                    }
                    let tx = Transaction::<D>::begin(&owner.database, owner.traces.root())?;
                    let bytes = tx.recall::<R>(&identity)?.map(|x| x.encode()).transpose()?;
                    let (sender, receiver) = watch::channel(bytes);
                    owner.subscribers.insert(key, sender);
                    Ok(Subscription {
                        receiver,
                        marker: std::marker::PhantomData,
                        lifecycle: None,
                    })
                })();
                let _ = reply.send(result);
            }))
            .map_err(|_| Failure::Stopped)?;
        wait.await.map_err(|_| Failure::Stopped)?
    }
}
pub trait Resuming<D: Domain> {
    fn resume_records<R: Resumable<D>>(
        &self,
    ) -> impl Future<Output = Result<(), D::Refusal>> + Send
    where
        R::Outcome: ResumeResult<D>;
}
impl<D: Domain> Resuming<D> for Scope<D>
where
    D::Refusal: TraceOutcome<D>,
{
    async fn resume_records<R: Resumable<D>>(&self) -> Result<(), D::Refusal>
    where
        R::Outcome: ResumeResult<D>,
    {
        let (reply, wait) = oneshot::channel();
        self.sender
            .send(Box::new(move |owner| {
                let result = Transaction::<D>::begin(&owner.database, owner.traces.root())
                    .and_then(|tx| tx.records::<R>());
                let _ = reply.send(result);
            }))
            .map_err(|_| D::Refusal::from(Failure::Stopped))?;
        let records = wait
            .await
            .map_err(|_| D::Refusal::from(Failure::Stopped))?
            .map_err(D::Refusal::from)?;
        for record in records {
            self.attempt(record).await.resumed()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(
    feature = "datom",
    derive(datom_codec::Datomizable, datom_codec::Composing)
)]
pub enum Boundary {
    AfterOutput,
    BeforeReserve,
    BeforeLock,
    AfterLock,
    Reserved,
    BeforeCall,
    BeforeJournal,
    AfterJournal,
    AfterReply,
    BeforeSettle,
    AfterSettle,
}
wire!(Boundary);
pub trait BoundaryReaching {
    fn boundary(&self, boundary: Boundary) -> impl Future<Output = Result<(), Failure>> + Send;
}
impl<D: Domain> BoundaryReaching for Scope<D> {
    async fn boundary(&self, boundary: Boundary) -> Result<(), Failure> {
        let configuration = self.configuration.lock().unwrap().clone();
        if configuration.pause_at.as_ref() == Some(&boundary) {
            let mut stream = tokio::net::UnixStream::connect(&configuration.observer_socket)
                .await
                .map_err(|_| Failure::Stopped)?;
            stream
                .write_frame(&boundary.encode()?, configuration.max_frame)
                .await?;
            stream.read_frame(configuration.max_frame).await?;
        }
        Ok(())
    }
}

pub trait ResumeResult<D: Domain> {
    fn resumed(self) -> Result<(), D::Refusal>;
}
impl<D: Domain, T> ResumeResult<D> for Result<T, D::Refusal> {
    fn resumed(self) -> Result<(), D::Refusal> {
        self.map(|_| ())
    }
}

pub trait ScopeClosing {
    fn shutdown(&self) -> impl Future<Output = ()> + Send;
}
impl<D: Domain> ScopeClosing for Scope<D> {
    async fn shutdown(&self) {
        let mut dead = self.dead.clone();
        let _ = self.sender.send(Box::new(|owner| owner.stopped = true));
        if !*dead.borrow() {
            let _ = dead.changed().await;
        }
    }
}
