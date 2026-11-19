//! Optional textual codec. Runtime octets and counts have checked datom views.
use crate::{Configuration, Failure, MetaInput, MetaOutput};
use datom_codec::{Budget, Composing, Datom, Datomizable, Error, ErrorKind, ErrorRaising};
#[derive(Composing, Datomizable)]
enum VisibleFailure {
    Unstored,
    Stopped,
    Wire,
    Schema {
        stored: Vec<i64>,
        expected: Vec<i64>,
    },
    Limit,
}
impl From<&Failure> for VisibleFailure {
    fn from(value: &Failure) -> Self {
        match value {
            Failure::Unstored => Self::Unstored,
            Failure::Stopped => Self::Stopped,
            Failure::Wire => Self::Wire,
            Failure::Limit => Self::Limit,
            Failure::Schema { stored, expected } => Self::Schema {
                stored: stored.iter().map(|v| i64::from(*v)).collect(),
                expected: expected.iter().map(|v| i64::from(*v)).collect(),
            },
        }
    }
}
impl Datomizable for Failure {
    fn datomize(&self, at: Vec<i64>) -> Datom {
        VisibleFailure::from(self).datomize(at)
    }
}
impl Composing for Failure {
    fn compose(datom: &Datom, budget: &mut Budget) -> Result<Self, Error> {
        let value = VisibleFailure::compose(datom, budget)?;
        let convert = |bytes: Vec<i64>| -> Result<[u8; 32], Error> {
            bytes
                .iter()
                .map(|v| u8::try_from(*v))
                .collect::<Result<Vec<_>, _>>()
                .ok()
                .and_then(|v| v.try_into().ok())
                .ok_or_else(|| {
                    Error::composition(
                        datom.path.clone(),
                        ErrorKind::Value {
                            expected: "32 octets".into(),
                            value: format!("{bytes:?}"),
                        },
                    )
                })
        };
        Ok(match value {
            VisibleFailure::Unstored => Self::Unstored,
            VisibleFailure::Stopped => Self::Stopped,
            VisibleFailure::Wire => Self::Wire,
            VisibleFailure::Limit => Self::Limit,
            VisibleFailure::Schema { stored, expected } => Self::Schema {
                stored: convert(stored)?,
                expected: convert(expected)?,
            },
        })
    }
}
#[derive(Composing, Datomizable)]
struct VisibleConfiguration {
    max_frame: i64,
    command_capacity: i64,
    trace_capacity: i64,
    max_subscriptions: i64,
    lock_socket: String,
    translator_socket: String,
    observer_socket: String,
    pause_at: Option<crate::Boundary>,
}
impl From<&Configuration> for VisibleConfiguration {
    fn from(c: &Configuration) -> Self {
        Self {
            max_frame: c.max_frame.into(),
            command_capacity: c.command_capacity.into(),
            trace_capacity: c.trace_capacity.into(),
            max_subscriptions: c.max_subscriptions.into(),
            lock_socket: c.lock_socket.clone(),
            translator_socket: c.translator_socket.clone(),
            observer_socket: c.observer_socket.clone(),
            pause_at: c.pause_at.clone(),
        }
    }
}
impl Datomizable for Configuration {
    fn datomize(&self, at: Vec<i64>) -> Datom {
        VisibleConfiguration::from(self).datomize(at)
    }
}
impl Composing for Configuration {
    fn compose(d: &Datom, b: &mut Budget) -> Result<Self, Error> {
        let c = VisibleConfiguration::compose(d, b)?;
        let count = |value| {
            u32::try_from(value).map_err(|_| {
                Error::composition(
                    d.path.clone(),
                    ErrorKind::Value {
                        expected: "unsigned 32-bit count".into(),
                        value: format!("{value}"),
                    },
                )
            })
        };
        Ok(Self {
            max_frame: count(c.max_frame)?,
            command_capacity: count(c.command_capacity)?,
            trace_capacity: count(c.trace_capacity)?,
            max_subscriptions: count(c.max_subscriptions)?,
            lock_socket: c.lock_socket,
            translator_socket: c.translator_socket,
            observer_socket: c.observer_socket,
            pause_at: c.pause_at,
        })
    }
}
#[derive(Composing, Datomizable)]
enum VisibleMetaInput {
    Configuration,
    MaxFrame(i64),
    CommandCapacity(i64),
    TraceCapacity(i64),
    MaxSubscriptions(i64),
    LockSocket(String),
    TranslatorSocket(String),
    FailCommit,
}
impl Composing for MetaInput {
    fn compose(d: &Datom, b: &mut Budget) -> Result<Self, Error> {
        let count = |value| {
            u32::try_from(value).map_err(|_| {
                Error::composition(
                    d.path.clone(),
                    ErrorKind::Value {
                        expected: "unsigned 32-bit count".into(),
                        value: format!("{value}"),
                    },
                )
            })
        };
        Ok(match VisibleMetaInput::compose(d, b)? {
            VisibleMetaInput::Configuration => Self::Configuration,
            VisibleMetaInput::MaxFrame(v) => Self::MaxFrame(count(v)?),
            VisibleMetaInput::CommandCapacity(v) => Self::CommandCapacity(count(v)?),
            VisibleMetaInput::TraceCapacity(v) => Self::TraceCapacity(count(v)?),
            VisibleMetaInput::MaxSubscriptions(v) => Self::MaxSubscriptions(count(v)?),
            VisibleMetaInput::LockSocket(v) => Self::LockSocket(v),
            VisibleMetaInput::TranslatorSocket(v) => Self::TranslatorSocket(v),
            VisibleMetaInput::FailCommit => Self::FailCommit,
        })
    }
}
#[derive(Datomizable)]
enum VisibleMetaOutput<'a> {
    Configured(&'a Configuration),
    Refused(&'a Failure),
}
impl Datomizable for MetaOutput {
    fn datomize(&self, at: Vec<i64>) -> Datom {
        match self {
            Self::Configured(v) => VisibleMetaOutput::Configured(v).datomize(at),
            Self::Refused(v) => VisibleMetaOutput::Refused(v).datomize(at),
        }
    }
}
