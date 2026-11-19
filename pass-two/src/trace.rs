//! Only tracing builds carry checkpoint values, paths, queues or sockets.
#[cfg(not(feature = "trace"))]
pub trait TraceObject {}
#[cfg(not(feature = "trace"))]
impl<T> TraceObject for T {}
#[cfg(not(feature = "trace"))]
pub trait TraceOutcome {}
#[cfg(not(feature = "trace"))]
impl<T> TraceOutcome for T {}
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg(feature = "trace")]
pub struct UpgradeObject {
    pub from: Vec<u8>,
    pub to: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct UpgradeYield {
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub records: u64,
}

#[cfg(feature = "trace")]
mod enabled {
    use super::{UpgradeObject, UpgradeYield};
    use crate::ledger::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    };
    use tokio::sync::broadcast;
    pub trait TraceObject {
        fn object(&self) -> Object;
    }
    pub trait TraceOutcome {
        fn outcome(&self) -> Outcome;
    }
    macro_rules! object {($($t:ident),*)=>{
 #[derive(Debug,Clone,PartialEq,Eq,rkyv::Archive,rkyv::Serialize,rkyv::Deserialize,datom_codec::Datomizable)]
 pub enum Object {$($t($t)),*,Upgrade(UpgradeObject),Subscribe(String)}
 $(impl TraceObject for $t {fn object(&self)->Object{Object::$t(self.clone())}})*
}}
    object!(
        Create,
        Deposit,
        Withdraw,
        Transfer,
        Reserve,
        Settle,
        Release,
        Inspect,
        Reservations,
        Pay,
        Resume,
        TimerFired,
        Remit,
        Status
    );
    macro_rules! outcome {($($t:ident),*)=>{
 #[derive(Debug,Clone,PartialEq,Eq,rkyv::Archive,rkyv::Serialize,rkyv::Deserialize,datom_codec::Datomizable)]
 pub enum Outcome {$($t($t)),*,Reservations(Vec<Reservation>),Refused(Failure),Upgrade(UpgradeYield),Subscribed(Account)}
 $(impl TraceOutcome for $t {fn outcome(&self)->Outcome{Outcome::$t(self.clone())}})*
}}
    outcome!(
        Created,
        Deposited,
        Withdrawn,
        Transferred,
        Reserved,
        Settled,
        Released,
        Account,
        RemitOutcome,
        PayOutcome,
        Resumed
    );
    impl TraceOutcome for UpgradeYield {
        fn outcome(&self) -> Outcome {
            Outcome::Upgrade(self.clone())
        }
    }
    impl TraceOutcome for Vec<Reservation> {
        fn outcome(&self) -> Outcome {
            Outcome::Reservations(self.clone())
        }
    }
    impl<T: TraceOutcome> TraceOutcome for Result<T, Failure> {
        fn outcome(&self) -> Outcome {
            match self {
                Ok(v) => v.outcome(),
                Err(e) => Outcome::Refused(e.clone()),
            }
        }
    }
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        rkyv::Archive,
        rkyv::Serialize,
        rkyv::Deserialize,
        datom_codec::Datomizable,
    )]
    pub enum Event {
        Entered(Object),
        Yielded(Outcome),
        Emitted(Outcome),
        Ended,
        Abandoned,
    }
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        rkyv::Archive,
        rkyv::Serialize,
        rkyv::Deserialize,
        datom_codec::Datomizable,
    )]
    pub struct Trace {
        pub root: i64,
        pub path: Vec<i64>,
        pub event: Event,
    }
    #[derive(Clone)]
    pub struct TraceHub {
        pub sender: broadcast::Sender<Trace>,
        pub history: Arc<Mutex<Vec<Trace>>>,
        next: Arc<AtomicI64>,
    }
    pub trait TracePublishing {
        fn publish(&self, trace: Trace);
        fn root(&self) -> TraceContext;
    }
    impl Default for TraceHub {
        fn default() -> Self {
            let (sender, _) = broadcast::channel(4096);
            Self {
                sender,
                history: Arc::default(),
                next: Arc::new(AtomicI64::new(0)),
            }
        }
    }
    #[derive(Clone, Default)]
    pub struct TraceContext {
        pub hub: TraceHub,
        pub root: i64,
        pub path: Vec<i64>,
        pub nested: bool,
        next: Arc<AtomicI64>,
    }
    pub trait TraceEntering {
        fn enter(&self, object: Object) -> TraceGuard;
        fn event(&self, event: Event);
    }
    pub struct TraceGuard {
        pub context: TraceContext,
        finished: bool,
    }
    pub trait TraceYielding {
        fn yielded(&mut self, outcome: Outcome);
    }
    impl TracePublishing for TraceHub {
        fn publish(&self, trace: Trace) {
            self.history.lock().unwrap().push(trace.clone());
            let _ = self.sender.send(trace);
        }
        fn root(&self) -> TraceContext {
            TraceContext {
                hub: self.clone(),
                root: self.next.fetch_add(1, Ordering::Relaxed),
                ..Default::default()
            }
        }
    }
    impl TraceEntering for TraceContext {
        fn enter(&self, object: Object) -> TraceGuard {
            let mut child = self.clone();
            if self.nested {
                child.path.push(self.next.fetch_add(1, Ordering::Relaxed));
            }
            child.nested = true;
            child.next = Arc::new(AtomicI64::new(0));
            child.event(Event::Entered(object));
            TraceGuard {
                context: child,
                finished: false,
            }
        }
        fn event(&self, event: Event) {
            self.hub.publish(Trace {
                root: self.root,
                path: self.path.clone(),
                event,
            });
        }
    }
    impl TraceYielding for TraceGuard {
        fn yielded(&mut self, outcome: Outcome) {
            self.context.event(Event::Yielded(outcome));
            self.finished = true;
        }
    }
    impl Drop for TraceGuard {
        fn drop(&mut self) {
            if !self.finished {
                self.context.event(Event::Abandoned);
            }
        }
    }
    // Byte fingerprints use integer positions at the text boundary; archive remains bytes.
    impl datom_codec::Datomizable for UpgradeObject {
        fn datomize(&self, at: datom_codec::Path) -> datom_codec::Datom {
            #[derive(datom_codec::Datomizable)]
            struct View {
                from: Vec<i64>,
                to: Vec<i64>,
            }
            View {
                from: self.from.iter().map(|x| i64::from(*x)).collect(),
                to: self.to.iter().map(|x| i64::from(*x)).collect(),
            }
            .datomize(at)
        }
    }
    impl datom_codec::Datomizable for UpgradeYield {
        fn datomize(&self, at: datom_codec::Path) -> datom_codec::Datom {
            #[derive(datom_codec::Datomizable)]
            struct View {
                from: Vec<i64>,
                to: Vec<i64>,
                records: i64,
            }
            View {
                from: self.from.iter().map(|x| i64::from(*x)).collect(),
                to: self.to.iter().map(|x| i64::from(*x)).collect(),
                records: self.records as i64,
            }
            .datomize(at)
        }
    }
}
#[cfg(feature = "trace")]
pub use enabled::*;
#[cfg(feature = "trace")]
pub trait TraceServing {
    fn serve(
        self,
        listener: tokio::net::UnixListener,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
#[cfg(feature = "trace")]
impl TraceServing for TraceHub {
    async fn serve(self, listener: tokio::net::UnixListener) -> anyhow::Result<()> {
        use crate::transport::FrameCarrying;
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
             connection=listener.accept()=>{let(mut stream,_)=connection?;let mut receiver=self.sender.subscribe();connections.spawn(async move{
              // Empty frame acknowledges an installed subscription before any root is sent.
              stream.write_frame(&[]).await?;
              loop{let trace=receiver.recv().await?;let bytes=rkyv::to_bytes::<rkyv::rancor::Error>(&trace)?;stream.write_frame(&bytes).await?;}
              #[allow(unreachable_code)]Ok::<_,anyhow::Error>(())
             });},
             _=connections.join_next(),if !connections.is_empty()=>{}
            }
        }
    }
}
