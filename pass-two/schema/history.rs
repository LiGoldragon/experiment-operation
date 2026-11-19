// Generated from the typed edit chain.
use super::{UpgradeInto,TableMigrating,MigrationObserver};
#[derive(Debug,Clone)]
pub struct Upgrade { pub from:Vec<u8>, pub to:Vec<u8>, pub fail_after:Option<usize> }
impl crate::Changeable for Upgrade {
    type Yield=crate::trace::UpgradeYield;
    type Refusal=crate::ledger::Failure;
    fn change(self,memory:&mut crate::memory::Transaction)->Result<Self::Yield,Self::Refusal> {
        super::UpgradeApplying::apply(self,memory)
    }
}
pub mod v0 {
#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    clippy::type_complexity
)]
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Account {
    pub string: String,
    pub integer: i64,
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Reservation {
    pub first_string: String,
    pub second_string: String,
    pub integer: i64,
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Detail {
    pub string: String,
    pub integer: i64,
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub enum State {
    Active(Detail),
    Idle(String),
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Envelope {
    pub integer: i64,
    pub detail_vector: std::vec::Vec<Detail>,
    pub detail_option: std::option::Option<Detail>,
    pub detail: Detail,
    pub state: State,
}

}
impl crate::memory::Memorizable for v0::Account {
const TABLE: &'static str = "account";
fn identity(&self)->Vec<u8> { self.r#string.as_bytes().to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
impl crate::memory::Memorizable for v0::Reservation {
const TABLE: &'static str = "reservation";
fn identity(&self)->Vec<u8> { self.r#first_string.as_bytes().to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
impl crate::memory::Memorizable for v0::Detail {
const TABLE: &'static str = "detail";
fn identity(&self)->Vec<u8> { self.r#string.as_bytes().to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
impl crate::memory::Memorizable for v0::Envelope {
const TABLE: &'static str = "envelope";
fn identity(&self)->Vec<u8> { rkyv::to_bytes::<rkyv::rancor::Error>(&self.r#integer).expect("scalar identity").to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
pub mod v1 {
#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    clippy::type_complexity
)]
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Account {
    pub string: String,
    pub integer: i64,
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Reservation {
    pub first_string: String,
    pub second_string: String,
    pub integer: i64,
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Detail {
    pub string: String,
    pub integer: i64,
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub enum State {
    Active(Detail),
    Idle(String),
}
#[rustfmt::skip]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "datom", derive(datom_codec::Datomizable, datom_codec::Composing))]
pub struct Envelope {
    pub integer: i64,
    pub detail_vector: std::vec::Vec<Detail>,
    pub detail_option: std::option::Option<Detail>,
    pub detail: Detail,
    pub state: State,
    pub boolean: bool,
    pub string: String,
}

}
impl crate::memory::Memorizable for v1::Account {
const TABLE: &'static str = "account";
fn identity(&self)->Vec<u8> { self.r#string.as_bytes().to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
impl crate::memory::Memorizable for v1::Reservation {
const TABLE: &'static str = "reservation";
fn identity(&self)->Vec<u8> { self.r#first_string.as_bytes().to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
impl crate::memory::Memorizable for v1::Detail {
const TABLE: &'static str = "detail";
fn identity(&self)->Vec<u8> { self.r#string.as_bytes().to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
impl crate::memory::Memorizable for v1::Envelope {
const TABLE: &'static str = "envelope";
fn identity(&self)->Vec<u8> { rkyv::to_bytes::<rkyv::rancor::Error>(&self.r#integer).expect("scalar identity").to_vec() }
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> { rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> { rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }
}
pub const FINGERPRINTS: &[&str] = &["8098d335fd12e5a1b2c6e280923bff632f1f40c36677e2573588320e19e0b46e", "df0572b2f4c0626b40e55713190dbd65e3a6d7aea105ded06c73bf20c3129d67"];
impl UpgradeInto<v1::Account> for v0::Account { fn upgrade(self)->anyhow::Result<v1::Account> { let old=self;
Ok(v1::Account {
r#string: old.r#string,
r#integer: old.r#integer,
})
} }
impl UpgradeInto<v1::Reservation> for v0::Reservation { fn upgrade(self)->anyhow::Result<v1::Reservation> { let old=self;
Ok(v1::Reservation {
r#first_string: old.r#first_string,
r#second_string: old.r#second_string,
r#integer: old.r#integer,
})
} }
impl UpgradeInto<v1::Detail> for v0::Detail { fn upgrade(self)->anyhow::Result<v1::Detail> { let old=self;
Ok(v1::Detail {
r#string: old.r#string,
r#integer: old.r#integer,
})
} }
impl UpgradeInto<v1::State> for v0::State { fn upgrade(self)->anyhow::Result<v1::State> { let old=self;
Ok(match old {
v0::State::Active(value) => v1::State::Active(value.upgrade()?),
v0::State::Idle(value) => v1::State::Idle(value),
})
} }
impl UpgradeInto<v1::Envelope> for v0::Envelope { fn upgrade(self)->anyhow::Result<v1::Envelope> { let old=self;
Ok(v1::Envelope {
r#integer: old.r#integer,
r#detail_vector: old.r#detail_vector.into_iter().map(|item| item.upgrade()).collect::<anyhow::Result<Vec<_>>>()?,
r#detail_option: old.r#detail_option.map(|item| -> anyhow::Result<_> { item.upgrade() }).transpose()?,
r#detail: old.r#detail.upgrade()?,
r#state: old.r#state.upgrade()?,
r#boolean: true,
r#string: String::from("upgraded"),
})
} }
impl super::GeneratedMigrating for crate::memory::Transaction { fn migrate_generated(&mut self, from:usize, observer:&mut dyn MigrationObserver)->anyhow::Result<()> { let transaction=self; for step in from..1 { match step {
0 => {
transaction.migrate_table::<v0::Account,v1::Account>(observer)?;
transaction.migrate_table::<v0::Reservation,v1::Reservation>(observer)?;
transaction.migrate_table::<v0::Detail,v1::Detail>(observer)?;
transaction.migrate_table::<v0::Envelope,v1::Envelope>(observer)?;
},
_ => unreachable!(), } } Ok(()) } }
