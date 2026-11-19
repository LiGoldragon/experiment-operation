use crate::*;
use redb::{Database, ReadableTable, TableDefinition, WriteTransaction};
use std::collections::HashMap;
use std::marker::PhantomData;
const RECORDS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("records");
const HOLDINGS: TableDefinition<&[u8], u64> = TableDefinition::new("holdings");
const METADATA: TableDefinition<&str, &[u8]> = TableDefinition::new("metadata");
pub trait Memorizable: Wire {
    const TABLE: &'static str;
    fn identity(&self) -> Vec<u8>;
}
pub struct Transaction<D: Domain> {
    pub(crate) held_owner: Option<u64>,
    pub(crate) write: WriteTransaction,
    pub(crate) changed: HashMap<Vec<u8>, Option<Vec<u8>>>,
    pub(crate) refusal: Option<D::Refusal>,
    pub(crate) trace: TraceContext<D>,
    pub(crate) pending: Option<(TraceGuard<D>, D::Outcome)>,
}
pub trait RecordKey {
    fn key(identity: &[u8]) -> Vec<u8>;
}
impl<R: Memorizable> RecordKey for R {
    fn key(identity: &[u8]) -> Vec<u8> {
        let mut key = R::TABLE.as_bytes().to_vec();
        key.push(0);
        key.extend(identity);
        key
    }
}
pub trait TransactionOpening<D: Domain> {
    fn begin(database: &Database, trace: TraceContext<D>) -> Result<Transaction<D>, Failure>;
}
impl<D: Domain> TransactionOpening<D> for Transaction<D> {
    fn begin(database: &Database, trace: TraceContext<D>) -> Result<Self, Failure> {
        Ok(Self {
            held_owner: None,
            write: database.begin_write().map_err(|_| Failure::Unstored)?,
            changed: HashMap::new(),
            refusal: None,
            trace,
            pending: None,
        })
    }
}
pub trait SchemaChecking<D: Domain> {
    fn schema(&mut self) -> Result<(), Failure>;
}
impl<D: Domain> SchemaChecking<D> for Transaction<D> {
    fn schema(&mut self) -> Result<(), Failure> {
        let stored = {
            let table = self
                .write
                .open_table(METADATA)
                .map_err(|_| Failure::Unstored)?;
            table
                .get("fingerprint")
                .map_err(|_| Failure::Unstored)?
                .map(|v| v.value().to_vec())
        };
        if let Some(stored) = stored {
            let stored: [u8; 32] = stored.try_into().map_err(|_| Failure::Wire)?;
            if stored != D::MEMORY_FINGERPRINT {
                D::upgrade(self, stored)?;
            }
        } else {
            D::upgrade(self, [0; 32])?;
        }
        self.write
            .open_table(METADATA)
            .map_err(|_| Failure::Unstored)?
            .insert("fingerprint", D::MEMORY_FINGERPRINT.as_slice())
            .map_err(|_| Failure::Unstored)?;
        Ok(())
    }
}
pub trait TransactionAccess {
    fn recall<R: Memorizable>(&self, identity: &[u8]) -> Result<Option<R>, Failure>;
    fn remember<R: Memorizable>(&mut self, record: R) -> Result<(), Failure>;
    fn forget<R: Memorizable>(&mut self, identity: &[u8]) -> Result<(), Failure>;
    fn records<R: Memorizable>(&self) -> Result<Vec<R>, Failure>;
}
impl<D: Domain> TransactionAccess for Transaction<D> {
    fn recall<R: Memorizable>(&self, identity: &[u8]) -> Result<Option<R>, Failure> {
        self.write
            .open_table(RECORDS)
            .map_err(|_| Failure::Unstored)?
            .get(R::key(identity).as_slice())
            .map_err(|_| Failure::Unstored)?
            .map(|v| R::decode(v.value()))
            .transpose()
    }
    fn remember<R: Memorizable>(&mut self, record: R) -> Result<(), Failure> {
        let key = R::key(&record.identity());
        let bytes = record.encode()?;
        self.write
            .open_table(RECORDS)
            .map_err(|_| Failure::Unstored)?
            .insert(key.as_slice(), bytes.as_slice())
            .map_err(|_| Failure::Unstored)?;
        self.changed.insert(key, Some(bytes));
        Ok(())
    }
    fn forget<R: Memorizable>(&mut self, identity: &[u8]) -> Result<(), Failure> {
        let key = R::key(identity);
        self.write
            .open_table(RECORDS)
            .map_err(|_| Failure::Unstored)?
            .remove(key.as_slice())
            .map_err(|_| Failure::Unstored)?;
        self.changed.insert(key, None);
        Ok(())
    }
    fn records<R: Memorizable>(&self) -> Result<Vec<R>, Failure> {
        let table = self
            .write
            .open_table(RECORDS)
            .map_err(|_| Failure::Unstored)?;
        let prefix = R::key(&[]);
        let mut records = Vec::new();
        for entry in table.iter().map_err(|_| Failure::Unstored)? {
            let (key, value) = entry.map_err(|_| Failure::Unstored)?;
            if key.value().starts_with(&prefix) {
                let record = R::decode(value.value())?;
                if R::key(&record.identity()) != key.value() {
                    return Err(Failure::Wire);
                }
                records.push(record);
            }
        }
        Ok(records)
    }
}
pub trait TransactionRunning<D: Domain> {
    fn run<C: Changeable<D>>(&mut self, change: C) -> Result<C::Yield, D::Refusal>;
}
impl<D: Domain> TransactionRunning<D> for Transaction<D>
where
    D::Refusal: TraceOutcome<D>,
{
    fn run<C: Changeable<D>>(&mut self, change: C) -> Result<C::Yield, D::Refusal> {
        let root = !self.trace.nested;
        let mut guard = self.trace.enter(change.object());
        let parent = std::mem::replace(&mut self.trace, guard.context.clone());
        let result = change.change(self);
        let result = match &self.refusal {
            Some(r) => Err(r.clone()),
            None => result,
        };
        if let Err(ref refusal) = result {
            self.refusal = Some(refusal.clone());
        }
        self.trace = parent;
        if root {
            self.pending = Some((guard, result.outcome()));
        } else {
            guard.yielded(result.outcome());
        }
        result
    }
}
pub struct Subscription<R: Memorizable> {
    pub receiver: tokio::sync::watch::Receiver<Option<Vec<u8>>>,
    pub(crate) marker: PhantomData<R>,
    pub lifecycle: Option<SubscriptionLifecycle>,
}
pub trait SubscriptionReceiving<R: Memorizable> {
    fn latest(&mut self) -> Result<Option<R>, Failure>;
    fn changed(&mut self) -> impl std::future::Future<Output = Result<Option<R>, Failure>> + Send;
}
impl<R: Memorizable> SubscriptionReceiving<R> for Subscription<R> {
    fn latest(&mut self) -> Result<Option<R>, Failure> {
        let changed = self.receiver.has_changed().unwrap_or(false);
        let bytes = self.receiver.borrow_and_update();
        if changed && let (Some(lifecycle), Some(bytes)) = (&self.lifecycle, bytes.as_ref()) {
            lifecycle(SubscriptionEvent::Emitted(bytes));
        }
        bytes.as_ref().map(|v| R::decode(v)).transpose()
    }
    async fn changed(&mut self) -> Result<Option<R>, Failure> {
        self.receiver
            .changed()
            .await
            .map_err(|_| Failure::Stopped)?;
        self.latest()
    }
}
impl<D: Domain, R: Memorizable + TraceOutcome<D>> TraceOutcome<D> for Subscription<R> {
    fn outcome(&self) -> D::Outcome {
        match self
            .receiver
            .borrow()
            .as_ref()
            .map(|v| R::decode(v))
            .transpose()
        {
            Ok(Some(record)) => record.outcome(),
            _ => Failure::Wire.into(),
        }
    }
    fn context(&mut self, context: TraceContext<D>) {
        self.lifecycle = Some(std::sync::Arc::new(move |event| match event {
            SubscriptionEvent::Emitted(bytes) => {
                if let Ok(record) = R::decode(bytes) {
                    context.event(Event::Emitted(record.outcome()));
                }
            }
            SubscriptionEvent::Ended => context.event(Event::Ended),
        }));
    }
}

pub enum SubscriptionEvent<'a> {
    Emitted(&'a [u8]),
    Ended,
}
pub type SubscriptionLifecycle = std::sync::Arc<dyn Fn(SubscriptionEvent<'_>) + Send + Sync>;
pub trait SubscriptionStreaming<R: Memorizable> {
    fn delivery<O: Wire>(self, map: fn(&[u8]) -> Result<O, Failure>) -> Delivery<O>;
}
impl<R: Memorizable> SubscriptionStreaming<R> for Subscription<R> {
    fn delivery<O: Wire>(mut self, map: fn(&[u8]) -> Result<O, Failure>) -> Delivery<O> {
        Delivery::Stream {
            receiver: self.receiver.clone(),
            map,
            lifecycle: self.lifecycle.take(),
        }
    }
}
impl<R: Memorizable> Drop for Subscription<R> {
    fn drop(&mut self) {
        if let Some(lifecycle) = &self.lifecycle {
            lifecycle(SubscriptionEvent::Ended);
        }
    }
}

pub trait TransactionCommitting {
    fn commit(self) -> Result<(), Failure>;
    fn abort(self, failure: Failure) -> Failure;
}
impl<D: Domain> TransactionCommitting for Transaction<D> {
    fn commit(self) -> Result<(), Failure> {
        let result = if self.refusal.is_some() {
            Err(Failure::Unstored)
        } else {
            self.write.commit().map_err(|_| Failure::Unstored)
        };
        if let Some((mut guard, outcome)) = self.pending {
            guard.yielded(match &result {
                Ok(()) => outcome,
                Err(failure) => failure.clone().into(),
            });
        }
        result
    }
    fn abort(self, failure: Failure) -> Failure {
        if let Some((mut guard, _)) = self.pending {
            guard.yielded(failure.clone().into());
        }
        failure
    }
}

impl<D: Domain> Transaction<D> {
    pub fn remember_held<R: Memorizable>(&mut self, record: R) -> Result<(), Failure> {
        let owner = self.held_owner.ok_or(Failure::Stopped)?;
        let key = R::key(&record.identity());
        self.remember(record)?;
        self.write
            .open_table(HOLDINGS)
            .map_err(|_| Failure::Unstored)?
            .insert(key.as_slice(), owner)
            .map_err(|_| Failure::Unstored)?;
        Ok(())
    }
    pub(crate) fn end_holding(&mut self, owner: Option<u64>) -> Result<(), Failure> {
        let keys = {
            let holdings = self
                .write
                .open_table(HOLDINGS)
                .map_err(|_| Failure::Unstored)?;
            let mut keys = Vec::new();
            for item in holdings.iter().map_err(|_| Failure::Unstored)? {
                let (key, value) = item.map_err(|_| Failure::Unstored)?;
                if owner.is_none_or(|owner| owner == value.value()) {
                    keys.push(key.value().to_vec());
                }
            }
            keys
        };
        for key in keys {
            self.write
                .open_table(RECORDS)
                .map_err(|_| Failure::Unstored)?
                .remove(key.as_slice())
                .map_err(|_| Failure::Unstored)?;
            self.write
                .open_table(HOLDINGS)
                .map_err(|_| Failure::Unstored)?
                .remove(key.as_slice())
                .map_err(|_| Failure::Unstored)?;
            self.changed.insert(key, None);
        }
        Ok(())
    }
}
