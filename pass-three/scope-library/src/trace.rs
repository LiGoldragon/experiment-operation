use crate::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::broadcast;
#[derive(Clone)]
pub enum Event<D: Domain> {
    Entered(D::Object),
    Yielded(D::Outcome),
    Emitted(D::Outcome),
    Ended,
    Abandoned,
}
#[derive(Clone)]
pub struct Trace<D: Domain> {
    pub root: u64,
    pub path: Vec<u64>,
    pub event: Event<D>,
}
#[derive(Clone)]
pub struct TraceHub<D: Domain> {
    pub sender: broadcast::Sender<Trace<D>>,
    next: Arc<AtomicU64>,
}
#[derive(Clone)]
pub struct TraceContext<D: Domain> {
    pub hub: TraceHub<D>,
    pub root: u64,
    pub path: Vec<u64>,
    pub(crate) nested: bool,
    next: Arc<AtomicU64>,
}
pub struct TraceGuard<D: Domain> {
    pub context: TraceContext<D>,
    finished: bool,
}
pub trait TraceOpening<D: Domain> {
    fn open(capacity: usize) -> Self;
    fn root(&self) -> TraceContext<D>;
}
impl<D: Domain> TraceOpening<D> for TraceHub<D> {
    fn open(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            next: Arc::new(AtomicU64::new(0)),
        }
    }
    fn root(&self) -> TraceContext<D> {
        TraceContext {
            hub: self.clone(),
            root: self.next.fetch_add(1, Ordering::Relaxed),
            path: Vec::new(),
            nested: false,
            next: Arc::new(AtomicU64::new(0)),
        }
    }
}
pub trait TraceEntering<D: Domain> {
    fn enter(&self, object: D::Object) -> TraceGuard<D>;
    fn event(&self, event: Event<D>);
}
impl<D: Domain> TraceEntering<D> for TraceContext<D> {
    fn enter(&self, object: D::Object) -> TraceGuard<D> {
        let mut child = self.clone();
        if self.nested {
            child.path.push(self.next.fetch_add(1, Ordering::Relaxed));
        }
        child.nested = true;
        child.next = Arc::new(AtomicU64::new(0));
        child.event(Event::Entered(object));
        TraceGuard {
            context: child,
            finished: false,
        }
    }
    fn event(&self, event: Event<D>) {
        #[cfg(feature = "trace")]
        let _ = self.hub.sender.send(Trace {
            root: self.root,
            path: self.path.clone(),
            event,
        });
        #[cfg(not(feature = "trace"))]
        let _ = event;
    }
}
pub trait TraceYielding<D: Domain> {
    fn yielded(&mut self, outcome: D::Outcome);
}
impl<D: Domain> TraceYielding<D> for TraceGuard<D> {
    fn yielded(&mut self, outcome: D::Outcome) {
        self.context.event(Event::Yielded(outcome));
        self.finished = true;
    }
}
impl<D: Domain> Drop for TraceGuard<D> {
    fn drop(&mut self) {
        if !self.finished {
            self.context.event(Event::Abandoned);
        }
    }
}
