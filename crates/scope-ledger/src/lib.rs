//! Trait-first operation trial. Store ownership is the only backend variation.
use kameo::{
    actor::{ActorRef, Spawn},
    message::{Context, Message},
};
use redb::{Database, ReadableTable, TableDefinition};
#[cfg(feature = "trace")]
use std::sync::Arc;
use std::{future::Future, marker::PhantomData, path::Path};
use tokio::sync::{mpsc, oneshot};

pub trait Textualizable {
    fn datom(&self) -> String;
}
pub trait Operable: Textualizable + Send {
    type Outcome: Textualizable + Send;
    fn operate(self, scope: &Scope) -> impl Future<Output = Self::Outcome> + Send;
}
pub trait Runnable {
    fn run<O: Operable>(&self, operation: O) -> impl Future<Output = O::Outcome> + Send;
}
pub trait Memorizable: Textualizable + Send + Sized {
    const TABLE: &'static str;
    fn identity(&self) -> String;
    fn encode(&self) -> Result<Vec<u8>, Failure>;
    fn decode(bytes: &[u8]) -> Result<Self, Failure>;
}
pub trait Dispatchable {
    fn dispatch(&self, command: Command) -> impl Future<Output = Result<Reply, Failure>> + Send;
    fn shutdown(&self) -> impl Future<Output = ()> + Send;
}
pub trait Executable {
    fn execute(&self, command: Command) -> Result<Reply, Failure>;
}
pub trait Openable: Sized {
    fn open(path: &Path, variant: Variant, capacity: usize) -> Result<Self, Failure>;
}
pub trait Traceable {
    fn trace(&self) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Storage(String),
    Stopped,
    InvalidReply,
    Missing(String),
    Insufficient(i64),
    InvalidAmount,
    Overflow,
    Rejected,
}
#[derive(Clone, Copy, Debug)]
pub enum Variant {
    Tasks,
    Actors,
}
#[derive(Debug)]
pub enum Command {
    Stop,
    Recall { table: &'static str, key: String },
    Atomic(Vec<Change>),
}
#[derive(Debug)]
pub enum Change {
    Remember {
        table: &'static str,
        key: String,
        bytes: Vec<u8>,
    },
    Forget {
        table: &'static str,
        key: String,
    },
    Reject,
}
#[derive(Debug)]
pub enum Reply {
    Recalled(Option<Vec<u8>>),
    Changed,
    Rejected,
}
struct Envelope {
    command: Command,
    reply: oneshot::Sender<Result<Reply, Failure>>,
}
#[derive(kameo::Actor)]
struct Store {
    database: Database,
}
#[derive(Clone)]
enum Backend {
    Tasks(mpsc::Sender<Envelope>),
    Actors(ActorRef<Store>),
}
#[derive(Clone)]
pub struct Scope {
    backend: Backend,
    #[cfg(feature = "trace")]
    traces: Arc<std::sync::Mutex<Vec<String>>>,
    #[cfg(feature = "trace")]
    path: Vec<usize>,
    #[cfg(feature = "trace")]
    nested: bool,
    #[cfg(feature = "trace")]
    next: Arc<std::sync::atomic::AtomicUsize>,
}

// Contracts above precede implementation bodies.
impl Openable for Scope {
    fn open(path: &Path, variant: Variant, capacity: usize) -> Result<Self, Failure> {
        let store = Store {
            database: Database::create(path).map_err(|e| Failure::Storage(e.to_string()))?,
        };
        let backend = match variant {
            Variant::Tasks => {
                let (sender, mut receiver) = mpsc::channel::<Envelope>(capacity);
                tokio::spawn(async move {
                    while let Some(envelope) = receiver.recv().await {
                        if matches!(envelope.command, Command::Stop) {
                            drop(store);
                            let _ = envelope.reply.send(Ok(Reply::Changed));
                            break;
                        }
                        let _ = envelope.reply.send(store.execute(envelope.command));
                    }
                });
                Backend::Tasks(sender)
            }
            Variant::Actors => Backend::Actors(Store::spawn(store)),
        };
        Ok(Self {
            backend,
            #[cfg(feature = "trace")]
            traces: Arc::default(),
            #[cfg(feature = "trace")]
            path: Vec::new(),
            #[cfg(feature = "trace")]
            nested: false,
            #[cfg(feature = "trace")]
            next: Arc::default(),
        })
    }
}
impl Runnable for Scope {
    async fn run<O: Operable>(&self, operation: O) -> O::Outcome {
        #[cfg(feature = "trace")]
        let child = {
            let mut path = self.path.clone();
            // A root is a fresh Scope clone; nested runs share their parent's child counter.
            if self.nested {
                path.push(self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            }
            let scope = Self {
                backend: self.backend.clone(),
                traces: self.traces.clone(),
                path,
                nested: true,
                next: Arc::default(),
            };
            scope.traces.lock().unwrap().push(format!(
                "{{ [ {} ] Entered.{} }}",
                scope
                    .path
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                operation.datom()
            ));
            scope
        };
        #[cfg(feature = "trace")]
        let outcome = operation.operate(&child).await;
        #[cfg(not(feature = "trace"))]
        let outcome = operation.operate(self).await;
        #[cfg(feature = "trace")]
        child.traces.lock().unwrap().push(format!(
            "{{ [ {} ] Yielded.{} }}",
            child
                .path
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            outcome.datom()
        ));
        outcome
    }
}
impl Traceable for Scope {
    fn trace(&self) -> Vec<String> {
        #[cfg(feature = "trace")]
        {
            self.traces.lock().unwrap().clone()
        }
        #[cfg(not(feature = "trace"))]
        {
            Vec::new()
        }
    }
}
impl Dispatchable for Scope {
    async fn dispatch(&self, command: Command) -> Result<Reply, Failure> {
        match &self.backend {
            Backend::Tasks(sender) => {
                let (reply, receiver) = oneshot::channel();
                sender
                    .send(Envelope { command, reply })
                    .await
                    .map_err(|_| Failure::Stopped)?;
                receiver.await.map_err(|_| Failure::Stopped)?
            }
            Backend::Actors(actor) => actor.ask(command).await.map_err(|e| match e {
                kameo::error::SendError::HandlerError(error) => error,
                _ => Failure::Stopped,
            }),
        }
    }
    async fn shutdown(&self) {
        if let Backend::Actors(actor) = &self.backend {
            let _ = actor.stop_gracefully().await;
            actor.wait_for_shutdown().await;
        }
        if let Backend::Tasks(_) = &self.backend {
            let _ = self.dispatch(Command::Stop).await;
        }
    }
}
impl Message<Command> for Store {
    type Reply = Result<Reply, Failure>;
    async fn handle(
        &mut self,
        command: Command,
        _: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        self.execute(command)
    }
}
impl Executable for Store {
    fn execute(&self, command: Command) -> Result<Reply, Failure> {
        let action = || -> Result<Reply, Box<dyn std::error::Error>> {
            match command {
                Command::Stop => Ok(Reply::Changed),
                Command::Recall { table, key } => {
                    let transaction = self.database.begin_read()?;
                    match transaction.open_table(TableDefinition::<&str, &[u8]>::new(table)) {
                        Ok(table) => Ok(Reply::Recalled(
                            table.get(key.as_str())?.map(|v| v.value().to_vec()),
                        )),
                        Err(redb::TableError::TableDoesNotExist(_)) => Ok(Reply::Recalled(None)),
                        Err(error) => Err(error.into()),
                    }
                }
                Command::Atomic(changes) => {
                    let transaction = self.database.begin_write()?;
                    for change in changes {
                        match change {
                            Change::Remember { table, key, bytes } => {
                                transaction
                                    .open_table(TableDefinition::<&str, &[u8]>::new(table))?
                                    .insert(key.as_str(), bytes.as_slice())?;
                            }
                            Change::Forget { table, key } => {
                                transaction
                                    .open_table(TableDefinition::<&str, &[u8]>::new(table))?
                                    .remove(key.as_str())?;
                            }
                            Change::Reject => return Ok(Reply::Rejected),
                        }
                    }
                    transaction.commit()?;
                    Ok(Reply::Changed)
                }
            }
        };
        action().map_err(|e| Failure::Storage(e.to_string()))
    }
}

#[derive(Debug)]
pub struct Recall<R> {
    pub key: String,
    pub record: PhantomData<R>,
}
#[derive(Debug)]
pub struct Remember<R>(pub R);
#[derive(Debug)]
pub struct Forget<R> {
    pub key: String,
    pub record: PhantomData<R>,
}
#[derive(Debug)]
pub struct Atomic(pub Vec<Change>);
impl<R: Memorizable> Operable for Recall<R> {
    type Outcome = Result<Option<R>, Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        match scope
            .dispatch(Command::Recall {
                table: R::TABLE,
                key: self.key,
            })
            .await?
        {
            Reply::Recalled(bytes) => bytes.map(|b| R::decode(&b)).transpose(),
            _ => Err(Failure::InvalidReply),
        }
    }
}
impl<R: Memorizable> Operable for Remember<R> {
    type Outcome = Result<(), Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        scope
            .run(Atomic(vec![Change::Remember {
                table: R::TABLE,
                key: self.0.identity(),
                bytes: self.0.encode()?,
            }]))
            .await
    }
}
impl<R: Memorizable> Operable for Forget<R> {
    type Outcome = Result<(), Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        scope
            .run(Atomic(vec![Change::Forget {
                table: R::TABLE,
                key: self.key,
            }]))
            .await
    }
}
impl Operable for Atomic {
    type Outcome = Result<(), Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        match scope.dispatch(Command::Atomic(self.0)).await? {
            Reply::Changed => Ok(()),
            _ => Err(Failure::Rejected),
        }
    }
}
impl<R> Textualizable for Recall<R> {
    fn datom(&self) -> String {
        format!("Recall.«{}»", self.key)
    }
}
impl<R: Textualizable> Textualizable for Remember<R> {
    fn datom(&self) -> String {
        format!("Remember.{}", self.0.datom())
    }
}
impl<R> Textualizable for Forget<R> {
    fn datom(&self) -> String {
        format!("Forget.«{}»", self.key)
    }
}
impl Textualizable for Atomic {
    fn datom(&self) -> String {
        format!(
            "Atomic.[ {} ]",
            self.0
                .iter()
                .map(Textualizable::datom)
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}
impl Textualizable for Failure {
    fn datom(&self) -> String {
        format!("Failure.«{self:?}»")
    }
}
impl Textualizable for () {
    fn datom(&self) -> String {
        "Succeeded".into()
    }
}
impl<T: Textualizable> Textualizable for Option<T> {
    fn datom(&self) -> String {
        match self {
            Some(v) => format!("Some.{}", v.datom()),
            None => "None".into(),
        }
    }
}
impl<T: Textualizable> Textualizable for Result<T, Failure> {
    fn datom(&self) -> String {
        match self {
            Ok(v) => format!("Accepted.{}", v.datom()),
            Err(e) => format!("Refused.{}", e.datom()),
        }
    }
}

pub mod ledger;

impl Textualizable for Change {
    fn datom(&self) -> String {
        match self {
            Self::Remember { table, key, bytes } => format!(
                "Remember.{{ «{table}» «{key}» [ {} ] }}",
                bytes
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Forget { table, key } => format!("Forget.{{ «{table}» «{key}» }}"),
            Self::Reject => "Reject".into(),
        }
    }
}

#[cfg(test)]
mod tests;

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for Failure {}
