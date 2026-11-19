//! Synchronous memory capabilities: no Scope or external effect enters a transaction.
use crate::ledger::Failure;
use redb::{Database, ReadableTable, TableDefinition, WriteTransaction};
use std::path::PathBuf;

pub trait Memorizable: Sized {
    const TABLE: &'static str;
    fn identity(&self) -> Vec<u8>;
    fn encode(&self) -> Result<Vec<u8>, Failure>;
    fn decode(bytes: &[u8]) -> Result<Self, Failure>;
}
pub struct Store {
    pub database: Database,
}
pub struct Transaction {
    pub write: WriteTransaction,
    pub changed: Vec<crate::ledger::Account>,
    pub refusal: Option<Failure>,
    #[cfg(feature = "trace")]
    pub trace: crate::trace::TraceContext,
}
pub trait StoreOpening: Sized {
    fn open(path: PathBuf) -> Result<Self, Failure>;
}
pub trait StoreAccess {
    fn begin(&self) -> Result<Transaction, Failure>;
}
pub trait TransactionCommit {
    fn commit(self) -> Result<(), Failure>;
}
pub trait TransactionAccess {
    fn recall<R: Memorizable>(&self, key: &str) -> Result<Option<R>, Failure> {
        self.recall_key::<R>(key.as_bytes())
    }
    fn recall_key<R: Memorizable>(&self, key: &[u8]) -> Result<Option<R>, Failure>;
    fn remember<R: Memorizable>(&mut self, record: R) -> Result<(), Failure>;
    fn forget<R: Memorizable>(&mut self, key: &str) -> Result<(), Failure>;
    fn records<R: Memorizable>(&self) -> Result<Vec<R>, Failure>;
}
impl StoreOpening for Store {
    fn open(path: PathBuf) -> Result<Self, Failure> {
        Ok(Self {
            database: Database::create(path).map_err(|e| Failure::Storage(e.to_string()))?,
        })
    }
}
impl StoreAccess for Store {
    fn begin(&self) -> Result<Transaction, Failure> {
        Ok(Transaction {
            write: self
                .database
                .begin_write()
                .map_err(|e| Failure::Storage(e.to_string()))?,
            changed: Vec::new(),
            refusal: None,
            #[cfg(feature = "trace")]
            trace: Default::default(),
        })
    }
}
impl TransactionCommit for Transaction {
    fn commit(self) -> Result<(), Failure> {
        if let Some(refusal) = self.refusal {
            return Err(refusal);
        }
        self.write
            .commit()
            .map_err(|e| Failure::Storage(e.to_string()))
    }
}
impl TransactionAccess for Transaction {
    fn recall_key<R: Memorizable>(&self, key: &[u8]) -> Result<Option<R>, Failure> {
        let table = self
            .write
            .open_table(TableDefinition::<&[u8], &[u8]>::new(R::TABLE))
            .map_err(|e| Failure::Storage(e.to_string()))?;
        table
            .get(key)
            .map_err(|e| Failure::Storage(e.to_string()))?
            .map(|value| R::decode(value.value()))
            .transpose()
    }
    fn remember<R: Memorizable>(&mut self, record: R) -> Result<(), Failure> {
        let key = record.identity();
        let bytes = record.encode()?;
        self.write
            .open_table(TableDefinition::<&[u8], &[u8]>::new(R::TABLE))
            .map_err(|e| Failure::Storage(e.to_string()))?
            .insert(key.as_slice(), bytes.as_slice())
            .map_err(|e| Failure::Storage(e.to_string()))?;
        Ok(())
    }
    fn forget<R: Memorizable>(&mut self, key: &str) -> Result<(), Failure> {
        self.write
            .open_table(TableDefinition::<&[u8], &[u8]>::new(R::TABLE))
            .map_err(|e| Failure::Storage(e.to_string()))?
            .remove(key.as_bytes())
            .map_err(|e| Failure::Storage(e.to_string()))?;
        Ok(())
    }
    fn records<R: Memorizable>(&self) -> Result<Vec<R>, Failure> {
        let table = self
            .write
            .open_table(TableDefinition::<&[u8], &[u8]>::new(R::TABLE))
            .map_err(|e| Failure::Storage(e.to_string()))?;
        let mut values = Vec::new();
        for entry in table.iter().map_err(|e| Failure::Storage(e.to_string()))? {
            let (key, value) = entry.map_err(|e| Failure::Storage(e.to_string()))?;
            let record = R::decode(value.value())?;
            if record.identity() != key.value() {
                return Err(Failure::Storage("record identity differs from key".into()));
            }
            values.push(record);
        }
        Ok(values)
    }
}
