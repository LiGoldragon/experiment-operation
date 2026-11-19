//! One blocking store-owner thread. Attempts remain ordinary Tokio tasks.
use crate::ledger::*;
use crate::memory::{Store, StoreAccess, StoreOpening, TransactionAccess, TransactionCommit};
#[cfg(feature = "trace")]
use crate::trace::{
    TraceContext, TraceEntering, TraceHub, TraceObject, TraceOutcome, TracePublishing,
    TraceYielding,
};
use crate::transport::{
    Holder, HolderDispatch, HolderMode, HolderOpening, SubscriberCommand, Subscription,
    SubscriptionEntry,
};
use crate::upgrade::UpgradePreparing;
use crate::{Changeable, Operable, Runnable, ScopeObject, TransactionRunning};
use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{Notify, mpsc, oneshot};

type Job = Box<dyn FnOnce(&Store) + Send>;
enum OwnerCommand {
    Execute(Job),
    Stop(oneshot::Sender<()>),
}
#[derive(Clone)]
pub struct Scope {
    owner: std::sync::mpsc::Sender<OwnerCommand>,
    pub holder: Holder,
    pub translator: StubTranslator,
    next_subscription: Arc<AtomicU64>,
    #[cfg(feature = "trace")]
    pub traces: TraceHub,
    #[cfg(feature = "trace")]
    pub context: Option<TraceContext>,
}
#[derive(Clone)]
pub struct StubTranslator {
    pub behavior: RemitBehavior,
    pub journal: Arc<Mutex<HashMap<String, RemitOutcome>>>,
    pub reached: Arc<Notify>,
}
#[derive(Clone, Debug)]
pub enum RemitBehavior {
    Succeed,
    Fail(i64),
    Never,
}
impl Default for StubTranslator {
    fn default() -> Self {
        Self {
            behavior: RemitBehavior::Succeed,
            journal: Arc::default(),
            reached: Arc::default(),
        }
    }
}
pub trait ScopeOpening: Sized {
    fn open(
        path: PathBuf,
        mode: HolderMode,
        translator: StubTranslator,
    ) -> impl Future<Output = Result<Self, Failure>> + Send;
}
pub trait ChangeExecuting {
    fn change<C: Changeable<Refusal = Failure>>(
        &self,
        c: C,
    ) -> impl Future<Output = Result<C::Yield, Failure>> + Send;
}
pub trait AttemptExecuting {
    fn attempt<A: Operable>(&self, a: A) -> impl Future<Output = A::Outcome> + Send;
}
pub trait Translating {
    fn remit(&self, r: Remit) -> impl Future<Output = RemitOutcome> + Send;
    fn status(&self, s: Status) -> impl Future<Output = RemitOutcome> + Send;
    fn timer(
        &self,
        event: TimerFired,
        delay: std::time::Duration,
    ) -> tokio::task::JoinHandle<Result<Deposited, Failure>>;
}
pub trait ScopeClosing {
    fn shutdown(&self) -> impl Future<Output = ()> + Send;
}
pub trait Subscribing {
    fn subscribe(&self, name: String)
    -> impl Future<Output = Result<Subscription, Failure>> + Send;
}
pub trait StubCalling {
    fn remit(&self, r: Remit) -> impl Future<Output = RemitOutcome> + Send;
    fn status(&self, s: Status) -> RemitOutcome;
}
impl ScopeOpening for Scope {
    async fn open(
        path: PathBuf,
        mode: HolderMode,
        translator: StubTranslator,
    ) -> Result<Self, Failure> {
        let holder = Holder::open(mode);
        let (sender, receiver) = std::sync::mpsc::channel();
        let (ready, wait) = oneshot::channel();
        #[cfg(feature = "trace")]
        let traces = TraceHub::default();
        #[cfg(feature = "trace")]
        let startup_trace = traces.root();
        std::thread::spawn(move || {
            let opened = (|| {
                let store = Store::open(path)?;
                let mut tx = store.begin()?;
                #[cfg(feature = "trace")]
                {
                    tx.trace = startup_trace;
                }
                if let Some(upgrade) = tx.prepare_upgrade(None)? {
                    tx.run(upgrade)?;
                }
                tx.commit()?;
                Ok::<_, Failure>(store)
            })();
            match opened {
                Err(e) => {
                    let _ = ready.send(Err(e));
                }
                Ok(store) => {
                    if ready.send(Ok(())).is_err() {
                        return;
                    }
                    while let Ok(command) = receiver.recv() {
                        match command {
                            OwnerCommand::Execute(job) => job(&store),
                            OwnerCommand::Stop(reply) => {
                                drop(store);
                                let _ = reply.send(());
                                break;
                            }
                        }
                    }
                }
            }
        });
        if let Err(failure) = wait.await.unwrap_or(Err(Failure::Stopped)) {
            holder.close().await;
            return Err(failure);
        }
        let scope = Self {
            owner: sender,
            holder,
            translator,
            next_subscription: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "trace")]
            traces,
            #[cfg(feature = "trace")]
            context: None,
        };
        // Resume is a root attempt before the caller receives a ready Scope.
        if let Err(failure) = scope.run(Resume {}).await {
            scope.shutdown().await;
            return Err(failure);
        }
        Ok(scope)
    }
}
impl Runnable for Scope {
    async fn run<O: ScopeObject>(&self, o: O) -> O::Outcome {
        o.execute(self).await
    }
}
impl ChangeExecuting for Scope {
    async fn change<C: Changeable<Refusal = Failure>>(&self, c: C) -> Result<C::Yield, Failure> {
        let (reply, wait) = oneshot::channel();
        let holder = self.holder.clone();
        #[cfg(feature = "trace")]
        let context = self.context.clone().unwrap_or_else(|| self.traces.root());
        self.owner
            .send(OwnerCommand::Execute(Box::new(move |store| {
                let result = (|| {
                    let mut tx = store.begin()?;
                    #[cfg(feature = "trace")]
                    {
                        tx.trace = context;
                    }
                    let value = tx.run(c)?;
                    let changed = tx.changed.clone();
                    tx.commit()?;
                    // Publish final records, once per account, in commit order.
                    let mut final_records = HashMap::new();
                    for a in changed {
                        final_records.insert(a.name.clone(), a);
                    }
                    for a in final_records.into_values() {
                        let _ = holder.dispatch(SubscriberCommand::Changed(a));
                    }
                    Ok(value)
                })();
                let _ = reply.send(result);
            })))
            .map_err(|_| Failure::Stopped)?;
        wait.await.map_err(|_| Failure::Stopped)?
    }
}
impl AttemptExecuting for Scope {
    async fn attempt<A: Operable>(&self, a: A) -> A::Outcome {
        #[cfg(feature = "trace")]
        let mut guard = self
            .context
            .clone()
            .unwrap_or_else(|| self.traces.root())
            .enter(a.object());
        let child = self.clone();
        #[cfg(feature = "trace")]
        let child = Self {
            context: Some(guard.context.clone()),
            ..child
        };
        let outcome = a.operate(&child).await;
        #[cfg(feature = "trace")]
        guard.yielded(outcome.outcome());
        outcome
    }
}
macro_rules! changes {($($t:ty),*)=>{$(impl ScopeObject for $t {type Outcome=Result<<Self as Changeable>::Yield,Failure>;async fn execute(self,s:&Scope)->Self::Outcome{s.change(self).await}})*}}
changes!(
    Create,
    Deposit,
    Withdraw,
    Transfer,
    Reserve,
    Settle,
    Release,
    Inspect,
    Reservations
);
macro_rules! attempts {($($t:ty),*)=>{$(impl ScopeObject for $t {type Outcome=<Self as Operable>::Outcome;async fn execute(self,s:&Scope)->Self::Outcome{s.attempt(self).await}})*}}
attempts!(Pay, Resume, TimerFired);
impl StubCalling for StubTranslator {
    async fn remit(&self, r: Remit) -> RemitOutcome {
        let outcome = match self.behavior {
            RemitBehavior::Succeed => RemitOutcome::Remitted,
            RemitBehavior::Fail(code) => RemitOutcome::Unremitted(code),
            RemitBehavior::Never => {
                // The world records success, then its reply is lost. Resume must ask Status.
                self.journal
                    .lock()
                    .unwrap()
                    .insert(r.reservation.reservation_id, RemitOutcome::Remitted);
                self.reached.notify_one();
                return std::future::pending().await;
            }
        };
        self.journal
            .lock()
            .unwrap()
            .insert(r.reservation.reservation_id, outcome.clone());
        self.reached.notify_one();
        outcome
    }
    fn status(&self, s: Status) -> RemitOutcome {
        self.journal
            .lock()
            .unwrap()
            .get(&s.reservation_id)
            .cloned()
            .unwrap_or(RemitOutcome::Unremitted(404))
    }
}
impl Translating for Scope {
    async fn remit(&self, r: Remit) -> RemitOutcome {
        #[cfg(feature = "trace")]
        let mut guard = self
            .context
            .clone()
            .unwrap_or_else(|| self.traces.root())
            .enter(r.object());
        let result = self.translator.remit(r).await;
        #[cfg(feature = "trace")]
        guard.yielded(result.outcome());
        result
    }
    async fn status(&self, s: Status) -> RemitOutcome {
        #[cfg(feature = "trace")]
        let mut guard = self
            .context
            .clone()
            .unwrap_or_else(|| self.traces.root())
            .enter(s.object());
        let result = self.translator.status(s);
        #[cfg(feature = "trace")]
        guard.yielded(result.outcome());
        result
    }
    fn timer(
        &self,
        event: TimerFired,
        delay: std::time::Duration,
    ) -> tokio::task::JoinHandle<Result<Deposited, Failure>> {
        let scope = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // The stub translator crosses a checked archive boundary before creating a root.
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event)
                .map_err(|e| Failure::Storage(e.to_string()))?;
            let input = rkyv::from_bytes::<TimerFired, rkyv::rancor::Error>(&bytes)
                .map_err(|e| Failure::Storage(e.to_string()))?;
            scope.run(input).await
        })
    }
}
impl ScopeClosing for Scope {
    async fn shutdown(&self) {
        let (reply, wait) = oneshot::channel();
        if self.owner.send(OwnerCommand::Stop(reply)).is_ok() {
            let _ = wait.await;
        }
        self.holder.close().await;
    }
}
impl Subscribing for Scope {
    async fn subscribe(&self, name: String) -> Result<Subscription, Failure> {
        let (reply, wait) = oneshot::channel();
        let id = self.next_subscription.fetch_add(1, Ordering::Relaxed);
        let holder = self.holder.clone();
        #[cfg(feature = "trace")]
        let context = self.traces.root();
        self.owner
            .send(OwnerCommand::Execute(Box::new(move |store| {
                let result = (|| {
                    let tx = store.begin()?;
                    let account = tx.recall::<Account>(&name)?.ok_or(Failure::Missing(name))?;
                    let (sender, receiver) = mpsc::unbounded_channel();
                    let entry = SubscriptionEntry {
                        id,
                        account: account.clone(),
                        sender,
                        #[cfg(feature = "trace")]
                        context,
                    };
                    holder.dispatch(SubscriberCommand::Open(entry))?;
                    Ok(Subscription {
                        id,
                        receiver,
                        holder,
                    })
                })();
                let _ = reply.send(result);
            })))
            .map_err(|_| Failure::Stopped)?;
        wait.await.map_err(|_| Failure::Stopped)?
    }
}
