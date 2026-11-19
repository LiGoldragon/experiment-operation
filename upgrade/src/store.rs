use anyhow::{Result, ensure};
use redb::{Database, ReadableTable, TableDefinition, WriteTransaction};
use std::path::PathBuf;

const METADATA: TableDefinition<&str, u32> = TableDefinition::new("__memory_version");

/// A concrete archived historical type, checked on read with rkyv bytecheck.
pub trait ArchiveRecord: Sized {
    const TABLE: &'static str;
    fn identity(&self) -> &str;
    fn encode(&self) -> Result<Vec<u8>>;
    fn decode(bytes: &[u8]) -> Result<Self>;
}
pub trait UpgradeInto<T> { fn upgrade(self) -> Result<T>; }
pub trait MigrationObserver { fn wrote_record(&mut self) -> Result<()>; }
pub trait Migrating {
    fn target(&self) -> u32;
    fn migrate(&self, transaction: &WriteTransaction, from: u32, observer: &mut dyn MigrationObserver) -> Result<()>;
}
pub trait TableMigrating {
    fn migrate_table<Old, New>(&self, observer: &mut dyn MigrationObserver) -> Result<()>
    where Old: ArchiveRecord + UpgradeInto<New>, New: ArchiveRecord;
}
/// The one owner of a store. Opening is explicit; migration is one transaction.
pub struct Store { pub path: PathBuf, database: Database }
pub trait StoreOpening: Sized { fn open(path: PathBuf) -> Result<Self>; }
pub trait StoreAccess {
    fn initialize(&self) -> Result<()>;
    fn version(&self) -> Result<u32>;
    fn insert<R: ArchiveRecord>(&self, version: u32, record: &R) -> Result<()>;
    fn records<R: ArchiveRecord>(&self, version: u32) -> Result<Vec<R>>;
    fn upgrade(&self, plan: &dyn Migrating, observer: &mut dyn MigrationObserver) -> Result<()>;
}
/// Faults are injected after a write has happened inside the uncommitted transaction.
pub struct FaultInjection { pub fail_after: Option<usize>, pub writes: usize }
impl MigrationObserver for FaultInjection {
    fn wrote_record(&mut self) -> Result<()> {
        self.writes += 1;
        ensure!(self.fail_after != Some(self.writes), "injected failure after record write {}", self.writes);
        Ok(())
    }
}
impl TableMigrating for WriteTransaction {
    fn migrate_table<Old, New>(&self, observer: &mut dyn MigrationObserver) -> Result<()>
    where Old: ArchiveRecord + UpgradeInto<New>, New: ArchiveRecord {
        let source = TableDefinition::<&str, &[u8]>::new(Old::TABLE);
        let target = TableDefinition::<&str, &[u8]>::new(New::TABLE);
        let records = {
            let table = self.open_table(source)?;
            let mut records = vec![];
            for entry in table.iter()? {
                let (key, bytes) = entry?;
                let record = Old::decode(bytes.value())?;
                ensure!(key.value() == record.identity(), "stored identity does not match its key");
                records.push(record);
            }
            records
        };
        {
            let mut table = self.open_table(target)?;
            ensure!(Old::TABLE == New::TABLE || table.is_empty()?, "rename target table already holds records");
            for old in records {
                let identity = old.identity().to_owned();
                let new = old.upgrade()?;
                ensure!(new.identity() == identity, "upgrade changed record identity");
                let bytes = new.encode()?;
                table.insert(identity.as_str(), bytes.as_slice())?;
                observer.wrote_record()?;
            }
        }
        if Old::TABLE != New::TABLE { self.delete_table(source)?; }
        Ok(())
    }
}
impl StoreOpening for Store {
    fn open(path: PathBuf) -> Result<Self> { Ok(Self { database: Database::create(&path)?, path }) }
}
impl StoreAccess for Store {
    fn initialize(&self) -> Result<()> {
        let transaction = self.database.begin_write()?;
        // A caller must explicitly initialize an empty store. Never stamp unknown records.
        ensure!(transaction.list_tables()?.next().is_none(), "initialize requires an empty store");
        { transaction.open_table(METADATA)?.insert("version", 0)?; }
        transaction.commit()?;
        Ok(())
    }
    fn version(&self) -> Result<u32> {
        let transaction = self.database.begin_read()?;
        let table = transaction.open_table(METADATA)?;
        let version = table.get("version")?.ok_or_else(|| anyhow::anyhow!("missing version stamp"))?.value();
        Ok(version)
    }
    fn insert<R: ArchiveRecord>(&self, version: u32, record: &R) -> Result<()> {
        let transaction = self.database.begin_write()?;
        ensure!(transaction.open_table(METADATA)?.get("version")?.is_some_and(|v| v.value() == version), "wrong schema version");
        {
            let mut table = transaction.open_table(TableDefinition::<&str, &[u8]>::new(R::TABLE))?;
            let bytes = record.encode()?;
            table.insert(record.identity(), bytes.as_slice())?;
        }
        transaction.commit()?;
        Ok(())
    }
    fn records<R: ArchiveRecord>(&self, version: u32) -> Result<Vec<R>> {
        let transaction = self.database.begin_read()?;
        ensure!(transaction.open_table(METADATA)?.get("version")?.is_some_and(|v| v.value() == version), "wrong schema version");
        let table = transaction.open_table(TableDefinition::<&str, &[u8]>::new(R::TABLE))?;
        let mut records = vec![];
        for entry in table.iter()? { let (_, bytes) = entry?; records.push(R::decode(bytes.value())?); }
        Ok(records)
    }
    fn upgrade(&self, plan: &dyn Migrating, observer: &mut dyn MigrationObserver) -> Result<()> {
        let transaction = self.database.begin_write()?;
        let version = transaction.open_table(METADATA)?.get("version")?.ok_or_else(|| anyhow::anyhow!("missing version stamp"))?.value();
        plan.migrate(&transaction, version, observer)?;
        { transaction.open_table(METADATA)?.insert("version", plan.target())?; }
        transaction.commit()?;
        Ok(())
    }
}
