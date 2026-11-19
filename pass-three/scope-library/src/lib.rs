//! Shared execution anatomy; domain vocabulary and bindings are generated.
use std::future::Future;
pub mod held;
pub use held::*;
pub mod journal;
pub mod memory;
pub mod process;
pub mod scope;
pub mod trace;
pub mod transport;
pub use journal::*;
pub use memory::*;
pub use process::*;
pub use scope::*;
pub use trace::*;
pub use transport::*;
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Failure {
    Unstored,
    Stopped,
    Wire,
    Schema {
        stored: [u8; 32],
        expected: [u8; 32],
    },
    Limit,
}
pub trait Wire: Clone + Send + Sync + 'static {
    fn encode(&self) -> Result<Vec<u8>, Failure>;
    fn decode(bytes: &[u8]) -> Result<Self, Failure>;
}
pub trait Domain: Clone + Send + Sync + 'static {
    type Object: Wire;
    type Outcome: Wire + From<Failure>;
    type Refusal: Wire + From<Failure>;
    const MEMORY_FINGERPRINT: [u8; 32];
    const SIGNAL_FINGERPRINT: [u8; 32];
    fn upgrade(_memory: &mut Transaction<Self>, stored: [u8; 32]) -> Result<(), Failure>
    where
        Self: Sized,
    {
        if stored == [0; 32] {
            Ok(())
        } else {
            Err(Failure::Schema {
                stored,
                expected: Self::MEMORY_FINGERPRINT,
            })
        }
    }
}
pub trait TraceObject<D: Domain>: Send + 'static {
    fn object(&self) -> D::Object;
}
pub trait TraceOutcome<D: Domain>: Send + 'static {
    fn outcome(&self) -> D::Outcome;
    fn context(&mut self, _context: TraceContext<D>) {}
}
impl<D: Domain, T: TraceOutcome<D>> TraceOutcome<D> for Result<T, D::Refusal>
where
    D::Refusal: TraceOutcome<D>,
{
    fn outcome(&self) -> D::Outcome {
        match self {
            Ok(x) => x.outcome(),
            Err(x) => x.outcome(),
        }
    }
    fn context(&mut self, context: TraceContext<D>) {
        match self {
            Ok(x) => x.context(context),
            Err(x) => x.context(context),
        }
    }
}
pub trait Changeable<D: Domain>: TraceObject<D> {
    type Yield: TraceOutcome<D>;
    fn change(self, memory: &mut Transaction<D>) -> Result<Self::Yield, D::Refusal>;
}
pub trait Operable<D: Domain>: TraceObject<D> {
    type Outcome: TraceOutcome<D>;
    fn operate(self, scope: &Scope<D>) -> impl Future<Output = Self::Outcome> + Send;
}
pub trait Resumable<D: Domain>: Memorizable + Operable<D> {}
pub trait ScopeObject<D: Domain>: Send + 'static {
    type Outcome: Send;
    fn execute(self, scope: &Scope<D>) -> impl Future<Output = Self::Outcome> + Send;
}
pub trait Runnable<D: Domain> {
    fn run<O: ScopeObject<D>>(&self, object: O) -> impl Future<Output = O::Outcome> + Send;
}

#[cfg(feature = "datom")]
mod text;
