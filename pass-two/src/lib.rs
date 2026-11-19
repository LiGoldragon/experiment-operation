//! Pass two: synchronous transactions and asynchronous attempts.
use std::future::Future;
pub mod ledger;
pub mod memory;
pub mod scope;
pub mod trace;
pub mod transport;
pub mod upgrade;
use ledger::Failure;
use memory::Transaction;
use scope::Scope;

pub trait Changeable: trace::TraceObject + Send + 'static {
    type Yield: trace::TraceOutcome + Send + 'static;
    type Refusal;
    fn change(self, memory: &mut Transaction) -> Result<Self::Yield, Self::Refusal>;
}
pub trait TransactionRunning {
    fn run<C: Changeable<Refusal = Failure>>(&mut self, change: C) -> Result<C::Yield, Failure>;
}
pub trait Operable: trace::TraceObject + Send + 'static {
    type Outcome: trace::TraceOutcome + Send + 'static;
    fn operate(self, scope: &Scope) -> impl Future<Output = Self::Outcome> + Send;
}
pub trait ScopeObject: Send + 'static {
    type Outcome: Send + 'static;
    fn execute(self, scope: &Scope) -> impl Future<Output = Self::Outcome> + Send;
}
pub trait Runnable {
    fn run<O: ScopeObject>(&self, object: O) -> impl Future<Output = O::Outcome> + Send;
}
impl TransactionRunning for Transaction {
    fn run<C: Changeable<Refusal = Failure>>(&mut self, change: C) -> Result<C::Yield, Failure> {
        #[cfg(feature = "trace")]
        let mut guard = self.trace.enter(change.object());
        #[cfg(feature = "trace")]
        let parent = std::mem::replace(&mut self.trace, guard.context.clone());
        let result = change.change(self);
        let result = match &self.refusal {
            Some(failure) => Err(failure.clone()),
            None => result,
        };
        if let Err(ref failure) = result {
            self.refusal = Some(failure.clone());
        }
        #[cfg(feature = "trace")]
        {
            self.trace = parent;
            guard.yielded(result.outcome());
        }
        result
    }
}
#[cfg(feature = "trace")]
use trace::{TraceEntering, TraceOutcome, TraceYielding};
