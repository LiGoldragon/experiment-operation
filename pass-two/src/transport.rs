//! Subscription holder comparison: same state/transition body, task or Kameo mailbox.
use crate::ledger::{Account, Failure};
#[cfg(feature = "trace")]
use crate::trace::{Event, Object, Outcome, TraceContext, TraceEntering};
use kameo::{
    actor::{ActorRef, Spawn},
    message::{Context, Message},
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
#[derive(Clone, Copy, Debug)]
pub enum HolderMode {
    Task,
    Actor,
}
#[derive(Clone)]
pub enum Holder {
    Task(mpsc::UnboundedSender<SubscriberCommand>),
    Actor(ActorRef<Subscribers>),
}
pub struct SubscriptionEntry {
    pub id: u64,
    pub account: Account,
    pub sender: mpsc::UnboundedSender<Account>,
    #[cfg(feature = "trace")]
    pub context: TraceContext,
}
pub enum SubscriberCommand {
    Open(SubscriptionEntry),
    Changed(Account),
    Leave(u64),
    Stop(oneshot::Sender<()>),
}
#[derive(kameo::Actor, Default)]
pub struct Subscribers {
    entries: HashMap<u64, SubscriptionEntry>,
}
pub struct Subscription {
    pub id: u64,
    pub receiver: mpsc::UnboundedReceiver<Account>,
    pub holder: Holder,
}
pub trait HolderOpening {
    fn open(mode: HolderMode) -> Self;
}
pub trait SubscriberApplying {
    fn apply(&mut self, command: SubscriberCommand);
}
pub trait HolderDispatch {
    fn dispatch(&self, command: SubscriberCommand) -> Result<(), Failure>;
    fn close(&self) -> impl std::future::Future<Output = ()> + Send;
}
pub trait SubscriptionReceiving {
    fn next(&mut self) -> impl std::future::Future<Output = Option<Account>> + Send;
}
impl HolderOpening for Holder {
    fn open(mode: HolderMode) -> Self {
        match mode {
            HolderMode::Task => {
                let (sender, mut receiver) = mpsc::unbounded_channel();
                tokio::spawn(async move {
                    let mut state = Subscribers::default();
                    while let Some(command) = receiver.recv().await {
                        let stop = matches!(&command, SubscriberCommand::Stop(_));
                        state.apply(command);
                        if stop {
                            break;
                        }
                    }
                });
                Self::Task(sender)
            }
            HolderMode::Actor => Self::Actor(Subscribers::spawn_with_mailbox(
                Subscribers::default(),
                kameo::mailbox::unbounded(),
            )),
        }
    }
}
impl HolderDispatch for Holder {
    fn dispatch(&self, command: SubscriberCommand) -> Result<(), Failure> {
        match self {
            Self::Task(sender) => sender.send(command).map_err(|_| Failure::Stopped),
            Self::Actor(actor) => actor.tell(command).try_send().map_err(|_| Failure::Stopped),
        }
    }
    async fn close(&self) {
        let (reply, wait) = oneshot::channel();
        if self.dispatch(SubscriberCommand::Stop(reply)).is_ok() {
            let _ = wait.await;
        }
        if let Self::Actor(actor) = self {
            let _ = actor.stop_gracefully().await;
            actor.wait_for_shutdown().await;
        }
    }
}
impl SubscriberApplying for Subscribers {
    fn apply(&mut self, command: SubscriberCommand) {
        match command {
            SubscriberCommand::Open(entry) => {
                #[cfg(feature = "trace")]
                {
                    entry.context.event(Event::Entered(Object::Subscribe(
                        entry.account.name.clone(),
                    )));
                    entry
                        .context
                        .event(Event::Yielded(Outcome::Subscribed(entry.account.clone())));
                }
                let _ = entry.sender.send(entry.account.clone());
                self.entries.insert(entry.id, entry);
            }
            SubscriberCommand::Changed(account) => {
                self.entries.retain(|_, entry| {
                    if entry.account.name != account.name {
                        return true;
                    }
                    if entry.sender.send(account.clone()).is_err() {
                        #[cfg(feature = "trace")]
                        entry.context.event(Event::Ended);
                        return false;
                    }
                    #[cfg(feature = "trace")]
                    entry
                        .context
                        .event(Event::Emitted(Outcome::Account(account.clone())));
                    true
                });
            }
            SubscriberCommand::Leave(id) => {
                if let Some(entry) = self.entries.remove(&id) {
                    #[cfg(feature = "trace")]
                    entry.context.event(Event::Ended);
                    #[cfg(not(feature = "trace"))]
                    let _ = entry;
                }
            }
            SubscriberCommand::Stop(reply) => {
                for entry in self.entries.drain().map(|(_, v)| v) {
                    #[cfg(feature = "trace")]
                    entry.context.event(Event::Ended);
                    #[cfg(not(feature = "trace"))]
                    let _ = entry;
                }
                let _ = reply.send(());
            }
        }
    }
}
impl Message<SubscriberCommand> for Subscribers {
    type Reply = ();
    async fn handle(&mut self, command: SubscriberCommand, _: &mut Context<Self, Self::Reply>) {
        self.apply(command);
    }
}
impl SubscriptionReceiving for Subscription {
    async fn next(&mut self) -> Option<Account> {
        self.receiver.recv().await
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self.holder.dispatch(SubscriberCommand::Leave(self.id));
    }
}

/// Each socket frame is a length-prefixed checked little-endian rkyv archive.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Input {
    Create(crate::ledger::Create),
    Deposit(crate::ledger::Deposit),
    Transfer(crate::ledger::Transfer),
    Inspect(crate::ledger::Inspect),
    Pay(crate::ledger::Pay),
    Subscribe(String),
    Timer(crate::ledger::TimerFired),
}
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Output {
    Created,
    Deposited(i64),
    Transferred,
    Account(Account),
    Pay(crate::ledger::PayOutcome),
    Refused(Failure),
}
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum MetaInput {
    Stop,
    Configuration,
}
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum MetaOutput {
    Stopped,
    Configuration { socket: String, meta: String },
}
pub struct Server {
    pub scope: crate::scope::Scope,
    pub socket: std::path::PathBuf,
    pub meta: std::path::PathBuf,
    pub trace: std::path::PathBuf,
}
pub trait LedgerServing {
    fn serve(self) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
pub trait FrameCarrying {
    fn read_frame(&mut self) -> impl std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send;
    fn write_frame(
        &mut self,
        bytes: &[u8],
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
impl FrameCarrying for tokio::net::UnixStream {
    async fn read_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let length = self.read_u32_le().await?;
        let mut bytes = vec![0; length as usize];
        self.read_exact(&mut bytes).await?;
        Ok(bytes)
    }
    async fn write_frame(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        self.write_u32_le(bytes.len().try_into()?).await?;
        self.write_all(bytes).await?;
        Ok(())
    }
}
pub trait InputAnswering {
    fn answer(
        self,
        scope: &crate::scope::Scope,
    ) -> impl std::future::Future<Output = Output> + Send;
}
impl InputAnswering for Input {
    async fn answer(self, s: &crate::scope::Scope) -> Output {
        use crate::Runnable;
        match self {
            Self::Create(c) => match s.run(c).await {
                Ok(_) => Output::Created,
                Err(e) => Output::Refused(e),
            },
            Self::Deposit(c) => match s.run(c).await {
                Ok(v) => Output::Deposited(v.value),
                Err(e) => Output::Refused(e),
            },
            Self::Transfer(c) => match s.run(c).await {
                Ok(_) => Output::Transferred,
                Err(e) => Output::Refused(e),
            },
            Self::Inspect(c) => match s.run(c).await {
                Ok(v) => Output::Account(v),
                Err(e) => Output::Refused(e),
            },
            Self::Pay(a) => Output::Pay(s.run(a).await),
            Self::Timer(a) => match s.run(a).await {
                Ok(v) => Output::Deposited(v.value),
                Err(e) => Output::Refused(e),
            },
            Self::Subscribe(_) => Output::Refused(Failure::Storage(
                "subscription requires streaming connection".into(),
            )),
        }
    }
}
pub trait ConnectionServing {
    fn serve(
        self,
        scope: crate::scope::Scope,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
impl ConnectionServing for tokio::net::UnixStream {
    async fn serve(mut self, scope: crate::scope::Scope) -> anyhow::Result<()> {
        let bytes = self.read_frame().await?;
        let input = rkyv::from_bytes::<Input, rkyv::rancor::Error>(&bytes)?;
        if let Input::Subscribe(name) = input {
            use crate::scope::Subscribing;
            let mut subscription = scope
                .subscribe(name)
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            loop {
                tokio::select! {
                 value=subscription.next()=>{match value{Some(a)=>{let bytes=rkyv::to_bytes::<rkyv::rancor::Error>(&Output::Account(a))?;self.write_frame(&bytes).await?;},None=>return Ok(())}},
                 _=self.read_frame()=>return Ok(())
                }
            }
        }
        let output = input.answer(&scope).await;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&output)?;
        self.write_frame(&bytes).await
    }
}
impl LedgerServing for Server {
    async fn serve(self) -> anyhow::Result<()> {
        use crate::scope::ScopeClosing;
        let listener = tokio::net::UnixListener::bind(&self.socket)?;
        let meta = tokio::net::UnixListener::bind(&self.meta)?;
        #[cfg(feature = "trace")]
        let trace_task = {
            use crate::trace::TraceServing;
            let listener = tokio::net::UnixListener::bind(&self.trace)?;
            let hub = self.scope.traces.clone();
            tokio::spawn(async move { hub.serve(listener).await })
        };
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
             connection=listener.accept()=>{let(stream,_)=connection?;let scope=self.scope.clone();connections.spawn(async move{let _=ConnectionServing::serve(stream,scope).await;});},
             connection=meta.accept()=>{let(mut stream,_)=connection?;let bytes=stream.read_frame().await?;match rkyv::from_bytes::<MetaInput,rkyv::rancor::Error>(&bytes)?{
              MetaInput::Stop=>{let bytes=rkyv::to_bytes::<rkyv::rancor::Error>(&MetaOutput::Stopped)?;stream.write_frame(&bytes).await?;break;},
              MetaInput::Configuration=>{let bytes=rkyv::to_bytes::<rkyv::rancor::Error>(&MetaOutput::Configuration{socket:self.socket.display().to_string(),meta:self.meta.display().to_string()})?;stream.write_frame(&bytes).await?;}
             }},
             _=tokio::signal::ctrl_c()=>break,
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        self.scope.shutdown().await;
        #[cfg(feature = "trace")]
        trace_task.abort();
        std::fs::remove_file(self.socket)?;
        std::fs::remove_file(self.meta)?;
        #[cfg(feature = "trace")]
        std::fs::remove_file(self.trace)?;
        Ok(())
    }
}
