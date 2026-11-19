use crate::*;
use std::sync::Arc;

/// The value is usable only while its single owning guard remains alive.
pub struct Held<T> {
    pub value: T,
    guard: Box<dyn Send + Sync>,
    lifecycle: Option<Box<dyn Send + Sync>>,
}
impl<T> Held<T> {
    pub fn new(value: T, guard: impl Send + Sync + 'static) -> Self {
        Self {
            value,
            guard: Box::new(guard),
            lifecycle: None,
        }
    }
    pub fn map<U>(self, value: U) -> Held<U> {
        Held {
            value,
            guard: self.guard,
            lifecycle: self.lifecycle,
        }
    }
}
impl<D: Domain, T: TraceOutcome<D>> TraceOutcome<D> for Held<T> {
    fn outcome(&self) -> D::Outcome {
        self.value.outcome()
    }
    fn context(&mut self, context: TraceContext<D>) {
        self.lifecycle = Some(Box::new(HoldingLifecycle(context)));
    }
}
struct HoldingLifecycle<D: Domain>(TraceContext<D>);
impl<D: Domain> Drop for HoldingLifecycle<D> {
    fn drop(&mut self) {
        self.0.event(Event::Ended);
    }
}
struct RecordHolding<D: Domain> {
    scope: Scope<D>,
    owner: u64,
}
impl<D: Domain> Drop for RecordHolding<D> {
    fn drop(&mut self) {
        let owner = self.owner;
        let _ = self.scope.sender.send(Box::new(move |state| {
            let result = (|| {
                let mut transaction =
                    Transaction::<D>::begin(&state.database, state.traces.root())?;
                transaction.end_holding(Some(owner))?;
                let changed = transaction.changed.clone();
                transaction.commit()?;
                for (key, bytes) in changed {
                    if let Some(sender) = state.subscribers.get(&key) {
                        sender.send_replace(bytes);
                    }
                }
                Ok::<_, Failure>(())
            })();
            if result.is_err() {
                state.stopped = true;
            }
        }));
    }
}
pub trait Holding<D: Domain> {
    fn hold<C: Changeable<D>>(
        &self,
        change: C,
    ) -> impl std::future::Future<Output = Result<Held<C::Yield>, D::Refusal>> + Send;
}
impl<D: Domain> Holding<D> for Scope<D>
where
    D::Refusal: TraceOutcome<D>,
{
    async fn hold<C: Changeable<D>>(&self, change: C) -> Result<Held<C::Yield>, D::Refusal> {
        let owner = self
            .holding_sequence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let guard = RecordHolding {
            scope: self.clone(),
            owner,
        };
        let child = Scope {
            held_owner: Some(owner),
            ..self.clone()
        };
        let value = child.change(change).await?;
        Ok(Held::new(value, guard))
    }
}
pub trait Exchanging {
    fn exchange<I: Wire, O: Wire>(
        &self,
        path: &str,
        fingerprint: [u8; 32],
        input: I,
    ) -> impl std::future::Future<Output = Result<Held<O>, Failure>> + Send;
}
impl<D: Domain> Exchanging for Scope<D> {
    async fn exchange<I: Wire, O: Wire>(
        &self,
        path: &str,
        fingerprint: [u8; 32],
        input: I,
    ) -> Result<Held<O>, Failure> {
        let limit = self.configuration.lock().unwrap().max_frame;
        let mut stream = tokio::net::UnixStream::connect(path)
            .await
            .map_err(|_| Failure::Stopped)?;
        match stream.greet(fingerprint, limit).await? {
            Greeting::Greeted => (),
            Greeting::Refused { received, expected } => {
                return Err(Failure::Schema {
                    stored: received,
                    expected,
                });
            }
        }
        stream.write_frame(&input.encode()?, limit).await?;
        let value = O::decode(&stream.read_frame(limit).await?)?;
        Ok(Held::new(value, stream))
    }
}
/// One pending emission; backpressure follows the connection's own consumption.
pub type OutputSink = Arc<tokio::sync::Mutex<tokio::sync::mpsc::Sender<Vec<u8>>>>;
pub trait Emitting<D: Domain> {
    fn emit(
        &self,
        outcome: D::Outcome,
        bytes: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(), Failure>> + Send;
}
impl<D: Domain> Emitting<D> for Scope<D> {
    async fn emit(&self, outcome: D::Outcome, bytes: Vec<u8>) -> Result<(), Failure> {
        if let Some(sink) = &self.output_sink {
            sink.lock()
                .await
                .send(bytes)
                .await
                .map_err(|_| Failure::Stopped)?;
        }
        if let Some(context) = &self.context {
            context.event(Event::Emitted(outcome));
        }
        Ok(())
    }
}
