use crate::*;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::sync::Arc;
const JOURNAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("outcomes");
#[derive(Clone)]
pub struct Journal {
    pub database: Arc<Database>,
}
pub trait JournalOpening: Sized {
    fn open(path: &str) -> Result<Self, Failure>;
}
pub trait JournalAccess {
    fn outcome<O: Wire>(&self, key: &[u8]) -> Result<Option<O>, Failure>;
    fn record<O: Wire>(&self, key: &[u8], outcome: O) -> Result<O, Failure>;
    fn entries(&self) -> Result<u64, Failure>;
}
impl JournalOpening for Journal {
    fn open(path: &str) -> Result<Self, Failure> {
        Ok(Self {
            database: Arc::new(Database::create(path).map_err(|_| Failure::Unstored)?),
        })
    }
}
impl JournalAccess for Journal {
    fn outcome<O: Wire>(&self, key: &[u8]) -> Result<Option<O>, Failure> {
        let tx = self.database.begin_write().map_err(|_| Failure::Unstored)?;
        let table = tx.open_table(JOURNAL).map_err(|_| Failure::Unstored)?;
        table
            .get(key)
            .map_err(|_| Failure::Unstored)?
            .map(|v| O::decode(v.value()))
            .transpose()
    }
    fn record<O: Wire>(&self, key: &[u8], outcome: O) -> Result<O, Failure> {
        let tx = self.database.begin_write().map_err(|_| Failure::Unstored)?;
        let result = {
            let mut table = tx.open_table(JOURNAL).map_err(|_| Failure::Unstored)?;
            let existing = table
                .get(key)
                .map_err(|_| Failure::Unstored)?
                .map(|v| v.value().to_vec());
            match existing {
                Some(bytes) => O::decode(&bytes)?,
                None => {
                    table
                        .insert(key, outcome.encode()?.as_slice())
                        .map_err(|_| Failure::Unstored)?;
                    outcome
                }
            }
        };
        tx.commit().map_err(|_| Failure::Unstored)?;
        Ok(result)
    }
    fn entries(&self) -> Result<u64, Failure> {
        let tx = self.database.begin_write().map_err(|_| Failure::Unstored)?;
        tx.open_table(JOURNAL)
            .map_err(|_| Failure::Unstored)?
            .len()
            .map_err(|_| Failure::Unstored)
    }
}
