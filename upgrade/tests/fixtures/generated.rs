// Generated from the edit manifest. Historical types are retained.
use experiment_upgrade::{UpgradeInto, TableMigrating, Migrating, MigrationObserver};
pub mod v0 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Frozen, Closed }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Account {
pub r#name: String,
pub r#balance: i64,
pub r#status: Status,
}
impl experiment_upgrade::ArchiveRecord for Account {
const TABLE: &'static str = "Account";
const VERSION: u32 = 0;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v1 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Frozen, Closed }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Account {
pub r#name: String,
pub r#balance: i64,
pub r#status: Status,
pub r#limit: i64,
}
impl experiment_upgrade::ArchiveRecord for Account {
const TABLE: &'static str = "Account";
const VERSION: u32 = 1;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v2 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Frozen, Closed }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerAccount {
pub r#name: String,
pub r#balance: i64,
pub r#status: Status,
pub r#limit: i64,
}
impl experiment_upgrade::ArchiveRecord for LedgerAccount {
const TABLE: &'static str = "LedgerAccount";
const VERSION: u32 = 2;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v3 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Frozen, Closed }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerAccount {
pub r#name: String,
pub r#amount: i64,
pub r#status: Status,
pub r#limit: i64,
}
impl experiment_upgrade::ArchiveRecord for LedgerAccount {
const TABLE: &'static str = "LedgerAccount";
const VERSION: u32 = 3;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v4 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Frozen, Closed }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerAccount {
pub r#name: String,
pub r#amount: i64,
pub r#status: Status,
}
impl experiment_upgrade::ArchiveRecord for LedgerAccount {
const TABLE: &'static str = "LedgerAccount";
const VERSION: u32 = 4;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v5 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Frozen, Closed, Dormant }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerAccount {
pub r#name: String,
pub r#amount: i64,
pub r#status: Status,
}
impl experiment_upgrade::ArchiveRecord for LedgerAccount {
const TABLE: &'static str = "LedgerAccount";
const VERSION: u32 = 5;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v6 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Closed, Dormant }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerAccount {
pub r#name: String,
pub r#amount: i64,
pub r#status: Status,
}
impl experiment_upgrade::ArchiveRecord for LedgerAccount {
const TABLE: &'static str = "LedgerAccount";
const VERSION: u32 = 6;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
pub mod v7 {
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Status { Open, Closed, Dormant }
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerAccount {
pub r#name: String,
pub r#amount: i64,
pub r#status: Status,
pub r#note: String,
}
impl experiment_upgrade::ArchiveRecord for LedgerAccount {
const TABLE: &'static str = "LedgerAccount";
const VERSION: u32 = 7;
fn identity(&self) -> &str { &self.r#name }
fn encode(&self) -> anyhow::Result<Vec<u8>> { Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }
fn decode(bytes: &[u8]) -> anyhow::Result<Self> { Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }
}
}
impl UpgradeInto<v1::Status> for v0::Status {
fn upgrade(self) -> anyhow::Result<v1::Status> { Ok(match self {
v0::Status::Open => v1::Status::Open,
v0::Status::Frozen => v1::Status::Frozen,
v0::Status::Closed => v1::Status::Closed,
}) } }
impl UpgradeInto<v1::Account> for v0::Account {
fn upgrade(self) -> anyhow::Result<v1::Account> {
let old = self;
Ok(v1::Account {
r#name: old.r#name,
r#balance: old.r#balance,
r#status: old.r#status.upgrade()?,
r#limit: 100_i64,
})
} }
impl UpgradeInto<v2::Status> for v1::Status {
fn upgrade(self) -> anyhow::Result<v2::Status> { Ok(match self {
v1::Status::Open => v2::Status::Open,
v1::Status::Frozen => v2::Status::Frozen,
v1::Status::Closed => v2::Status::Closed,
}) } }
impl UpgradeInto<v2::LedgerAccount> for v1::Account {
fn upgrade(self) -> anyhow::Result<v2::LedgerAccount> {
let old = self;
Ok(v2::LedgerAccount {
r#name: old.r#name,
r#balance: old.r#balance,
r#status: old.r#status.upgrade()?,
r#limit: old.r#limit,
})
} }
impl UpgradeInto<v3::Status> for v2::Status {
fn upgrade(self) -> anyhow::Result<v3::Status> { Ok(match self {
v2::Status::Open => v3::Status::Open,
v2::Status::Frozen => v3::Status::Frozen,
v2::Status::Closed => v3::Status::Closed,
}) } }
impl UpgradeInto<v3::LedgerAccount> for v2::LedgerAccount {
fn upgrade(self) -> anyhow::Result<v3::LedgerAccount> {
let old = self;
Ok(v3::LedgerAccount {
r#name: old.r#name,
r#amount: old.r#balance,
r#status: old.r#status.upgrade()?,
r#limit: old.r#limit,
})
} }
impl UpgradeInto<v4::Status> for v3::Status {
fn upgrade(self) -> anyhow::Result<v4::Status> { Ok(match self {
v3::Status::Open => v4::Status::Open,
v3::Status::Frozen => v4::Status::Frozen,
v3::Status::Closed => v4::Status::Closed,
}) } }
impl UpgradeInto<v4::LedgerAccount> for v3::LedgerAccount {
fn upgrade(self) -> anyhow::Result<v4::LedgerAccount> {
let old = self;
Ok(v4::LedgerAccount {
r#name: old.r#name,
r#amount: old.r#amount,
r#status: old.r#status.upgrade()?,
})
} }
impl UpgradeInto<v5::Status> for v4::Status {
fn upgrade(self) -> anyhow::Result<v5::Status> { Ok(match self {
v4::Status::Open => v5::Status::Open,
v4::Status::Frozen => v5::Status::Frozen,
v4::Status::Closed => v5::Status::Closed,
}) } }
impl UpgradeInto<v5::LedgerAccount> for v4::LedgerAccount {
fn upgrade(self) -> anyhow::Result<v5::LedgerAccount> {
let old = self;
Ok(v5::LedgerAccount {
r#name: old.r#name,
r#amount: old.r#amount,
r#status: old.r#status.upgrade()?,
})
} }
impl UpgradeInto<v6::Status> for v5::Status {
fn upgrade(self) -> anyhow::Result<v6::Status> { Ok(match self {
v5::Status::Open => v6::Status::Open,
v5::Status::Frozen => v6::Status::Closed,
v5::Status::Closed => v6::Status::Closed,
v5::Status::Dormant => v6::Status::Dormant,
}) } }
impl UpgradeInto<v6::LedgerAccount> for v5::LedgerAccount {
fn upgrade(self) -> anyhow::Result<v6::LedgerAccount> {
let old = self;
Ok(v6::LedgerAccount {
r#name: old.r#name,
r#amount: old.r#amount,
r#status: old.r#status.upgrade()?,
})
} }
impl UpgradeInto<v7::Status> for v6::Status {
fn upgrade(self) -> anyhow::Result<v7::Status> { Ok(match self {
v6::Status::Open => v7::Status::Open,
v6::Status::Closed => v7::Status::Closed,
v6::Status::Dormant => v7::Status::Dormant,
}) } }
impl UpgradeInto<v7::LedgerAccount> for v6::LedgerAccount {
fn upgrade(self) -> anyhow::Result<v7::LedgerAccount> {
let old = self;
let new: v7::LedgerAccount = { v7::LedgerAccount { r#name: old.r#name, r#amount: old.r#amount + 1, r#status: old.r#status.upgrade()?, r#note: String::from("reshaped") } };
Ok(new)
} }
/// Selects a compiled target, allowing a store to stop at an intermediate version.
pub struct UpgradePlan { pub target: u32 }
impl Migrating for UpgradePlan {
fn target(&self) -> u32 { self.target }
fn migrate(&self, transaction: &redb::WriteTransaction, from: u32, observer: &mut dyn MigrationObserver) -> anyhow::Result<()> {
anyhow::ensure!(self.target <= 7, "target exceeds compiled manifest");
anyhow::ensure!(from <= self.target, "store is newer than target");
for version in from..self.target { match version {
0 => {
transaction.migrate_table::<v0::Account, v1::Account>(observer)?;
}
1 => {
transaction.migrate_table::<v1::Account, v2::LedgerAccount>(observer)?;
}
2 => {
transaction.migrate_table::<v2::LedgerAccount, v3::LedgerAccount>(observer)?;
}
3 => {
transaction.migrate_table::<v3::LedgerAccount, v4::LedgerAccount>(observer)?;
}
4 => {
transaction.migrate_table::<v4::LedgerAccount, v5::LedgerAccount>(observer)?;
}
5 => {
transaction.migrate_table::<v5::LedgerAccount, v6::LedgerAccount>(observer)?;
}
6 => {
transaction.migrate_table::<v6::LedgerAccount, v7::LedgerAccount>(observer)?;
}
_ => anyhow::bail!("unknown source version {version}"),
} }
Ok(())
} }
