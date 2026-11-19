//! The schema guard and generated migration are the store owner's first change.
use crate::trace::UpgradeYield;
use crate::{
    ledger::Failure,
    memory::{Memorizable, Transaction, TransactionAccess},
};
use anyhow::{Result, ensure};
use redb::{ReadableTable, ReadableTableMetadata, TableDefinition, TableHandle};

#[cfg(feature = "datom")]
#[path = "../schema/edit.rs"]
pub mod edit;
#[cfg(feature = "datom")]
#[path = "../schema/tool.rs"]
pub mod tool;
#[cfg(feature = "datom")]
pub use edit::*;
#[cfg(feature = "datom")]
pub use tool::*;
#[path = "../schema/history.rs"]
pub mod history;

pub const STAMP: TableDefinition<&str, &str> = TableDefinition::new("__memory_fingerprint");
pub trait UpgradeInto<T> {
    fn upgrade(self) -> Result<T>;
}
pub trait GeneratedMigrating {
    fn migrate_generated(
        &mut self,
        from: usize,
        observer: &mut dyn MigrationObserver,
    ) -> Result<()>;
}
pub trait MigrationObserver {
    fn wrote_record(&mut self) -> Result<()>;
}
pub struct FaultInjection {
    pub fail_after: Option<usize>,
    pub writes: usize,
}
pub trait TableMigrating {
    fn migrate_table<Old: Memorizable + UpgradeInto<New>, New: Memorizable>(
        &mut self,
        observer: &mut dyn MigrationObserver,
    ) -> Result<()>;
}
pub use history::Upgrade;
pub trait UpgradeApplying {
    fn apply(self, memory: &mut Transaction) -> std::result::Result<UpgradeYield, Failure>;
}
pub trait UpgradePreparing {
    fn prepare_upgrade(
        &self,
        fail_after: Option<usize>,
    ) -> std::result::Result<Option<Upgrade>, Failure>;
}
impl MigrationObserver for FaultInjection {
    fn wrote_record(&mut self) -> Result<()> {
        self.writes += 1;
        ensure!(
            self.fail_after != Some(self.writes),
            "injected migration failure after write {}",
            self.writes
        );
        Ok(())
    }
}
impl TableMigrating for Transaction {
    fn migrate_table<Old: Memorizable + UpgradeInto<New>, New: Memorizable>(
        &mut self,
        observer: &mut dyn MigrationObserver,
    ) -> Result<()> {
        let records = self
            .records::<Old>()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let target = TableDefinition::<&[u8], &[u8]>::new(New::TABLE);
        if Old::TABLE != New::TABLE {
            ensure!(
                self.write.open_table(target)?.is_empty()?,
                "target table is occupied"
            );
        }
        for old in records {
            let identity = old.identity();
            let new = old.upgrade()?;
            ensure!(identity == new.identity(), "migration changed identity");
            self.remember(new).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            observer.wrote_record()?;
        }
        if Old::TABLE != New::TABLE {
            self.write
                .delete_table(TableDefinition::<&[u8], &[u8]>::new(Old::TABLE))?;
        }
        Ok(())
    }
}
impl UpgradePreparing for Transaction {
    fn prepare_upgrade(
        &self,
        fail_after: Option<usize>,
    ) -> std::result::Result<Option<Upgrade>, Failure> {
        let expected = history::FINGERPRINTS
            .last()
            .ok_or_else(|| Failure::Storage("compiled chain is empty".into()))?;
        let names = self
            .write
            .list_tables()
            .map_err(|e| Failure::Storage(e.to_string()))?
            .map(|table| table.name().to_owned())
            .collect::<Vec<_>>();
        let stamp = if names.iter().any(|name| name == STAMP.name()) {
            self.write
                .open_table(STAMP)
                .map_err(|e| Failure::Storage(e.to_string()))?
                .get("schema")
                .map_err(|e| Failure::Storage(e.to_string()))?
                .map(|v| v.value().to_owned())
        } else {
            None
        };
        let from = match stamp {
            None if names.is_empty() => String::new(),
            None => {
                return Err(Failure::Storage(format!(
                    "unstamped nonempty store; expected {expected}"
                )));
            }
            Some(stamp) if stamp == *expected => return Ok(None),
            Some(stamp) if history::FINGERPRINTS.contains(&stamp.as_str()) => stamp,
            Some(stamp) => {
                return Err(Failure::Storage(format!(
                    "unknown Memory fingerprint {stamp}; expected {expected}"
                )));
            }
        };
        Ok(Some(Upgrade {
            from: from.into_bytes(),
            to: expected.as_bytes().to_vec(),
            fail_after,
        }))
    }
}
impl UpgradeApplying for Upgrade {
    fn apply(self, memory: &mut Transaction) -> std::result::Result<UpgradeYield, Failure> {
        let checked = memory.prepare_upgrade(self.fail_after)?;
        let Some(checked) = checked else {
            return Err(Failure::Storage("upgrade is no longer required".into()));
        };
        if checked.from != self.from || checked.to != self.to {
            return Err(Failure::Storage("upgrade stamps changed".into()));
        }
        let mut observer = FaultInjection {
            fail_after: self.fail_after,
            writes: 0,
        };
        if !self.from.is_empty() {
            let from = history::FINGERPRINTS
                .iter()
                .position(|stamp| stamp.as_bytes() == self.from)
                .ok_or_else(|| Failure::Storage("unknown source stamp".into()))?;
            memory
                .migrate_generated(from, &mut observer)
                .map_err(Failure::from)?;
        }
        let stamp = std::str::from_utf8(&self.to).map_err(|e| Failure::Storage(e.to_string()))?;
        memory
            .write
            .open_table(STAMP)
            .map_err(|e| Failure::Storage(e.to_string()))?
            .insert("schema", stamp)
            .map_err(|e| Failure::Storage(e.to_string()))?;
        if self.fail_after == Some(observer.writes + 1) {
            return Err(Failure::Storage(
                "injected migration failure after stamp write".into(),
            ));
        }
        Ok(UpgradeYield {
            from: self.from,
            to: self.to,
            records: observer.writes as u64,
        })
    }
}
#[cfg(feature = "trace")]
impl crate::trace::TraceObject for Upgrade {
    fn object(&self) -> crate::trace::Object {
        crate::trace::Object::Upgrade(crate::trace::UpgradeObject {
            from: self.from.clone(),
            to: self.to.clone(),
        })
    }
}
