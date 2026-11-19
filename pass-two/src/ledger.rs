//! Ledger changes carry no async capability; attempts carry only Scope.
use crate::memory::{Memorizable, Transaction, TransactionAccess};
use crate::scope::{Scope, Translating};
use crate::{Changeable, Operable, Runnable, TransactionRunning};
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(
    feature = "trace",
    derive(datom_codec::Composing, datom_codec::Datomizable)
)]
pub enum Failure {
    Storage(String),
    Missing(String),
    Insufficient(i64),
    InvalidAmount,
    Overflow,
    Duplicate(String),
    UnknownReservation(String),
    Stopped,
}
impl From<anyhow::Error> for Failure {
    fn from(e: anyhow::Error) -> Self {
        Self::Storage(e.to_string())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(
    feature = "trace",
    derive(datom_codec::Composing, datom_codec::Datomizable)
)]
pub struct Account {
    pub name: String,
    pub balance: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(
    feature = "trace",
    derive(datom_codec::Composing, datom_codec::Datomizable)
)]
pub struct Reservation {
    pub reservation_id: String,
    pub name: String,
    pub amount: i64,
}
impl Memorizable for Account {
    const TABLE: &'static str = "account";
    fn identity(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }
    fn encode(&self) -> Result<Vec<u8>, Failure> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map(|b| b.to_vec())
            .map_err(|e| Failure::Storage(e.to_string()))
    }
    fn decode(b: &[u8]) -> Result<Self, Failure> {
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(b)
            .map_err(|e| Failure::Storage(e.to_string()))
    }
}
impl Memorizable for Reservation {
    const TABLE: &'static str = "reservation";
    fn identity(&self) -> Vec<u8> {
        self.reservation_id.as_bytes().to_vec()
    }
    fn encode(&self) -> Result<Vec<u8>, Failure> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map(|b| b.to_vec())
            .map_err(|e| Failure::Storage(e.to_string()))
    }
    fn decode(b: &[u8]) -> Result<Self, Failure> {
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(b)
            .map_err(|e| Failure::Storage(e.to_string()))
    }
}
macro_rules! data {($n:ident {$($f:ident:$t:ty),*})=>{
 #[derive(Debug,Clone,PartialEq,Eq,rkyv::Archive,rkyv::Serialize,rkyv::Deserialize)]
 #[cfg_attr(feature="trace",derive(datom_codec::Composing,datom_codec::Datomizable))]
 pub struct $n {$(pub $f:$t),*}
}}
data!(Deposit {
    name: String,
    amount: i64
});
data!(Withdraw {
    name: String,
    amount: i64
});
data!(Transfer {
    from: String,
    to: String,
    amount: i64
});
data!(Reserve {
    reservation_id: String,
    name: String,
    amount: i64
});
data!(Settle {
    reservation_id: String
});
data!(Release {
    reservation_id: String
});
data!(Inspect { name: String });
data!(Reservations {});
data!(Create { account: Account });
data!(Pay {
    reservation_id: String,
    name: String,
    amount: i64
});
data!(Resume {});
data!(TimerFired {
    name: String,
    amount: i64
});
data!(Remit {
    reservation: Reservation
});
data!(Status {
    reservation_id: String
});
macro_rules! value {
    ($n:ident,$t:ty) => {
        data!($n { value: $t });
    };
}
value!(Deposited, i64);
value!(Withdrawn, i64);
data!(Transferred {});
data!(Reserved {});
data!(Settled {});
data!(Released {});
data!(Created {});
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(
    feature = "trace",
    derive(datom_codec::Composing, datom_codec::Datomizable)
)]
pub enum RemitOutcome {
    Remitted,
    Unremitted(i64),
}
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[cfg_attr(
    feature = "trace",
    derive(datom_codec::Composing, datom_codec::Datomizable)
)]
pub enum PayOutcome {
    Paid,
    Unpaid(i64),
    Refused(Failure),
}
value!(Resumed, i64);
impl Changeable for Create {
    type Yield = Created;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Created, Failure> {
        if m.recall::<Account>(&self.account.name)?.is_some() {
            return Err(Failure::Duplicate(self.account.name));
        }
        if self.account.balance < 0 {
            return Err(Failure::InvalidAmount);
        }
        m.remember(self.account.clone())?;
        m.changed.push(self.account);
        Ok(Created {})
    }
}
impl Changeable for Inspect {
    type Yield = Account;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Account, Failure> {
        m.recall::<Account>(&self.name)?
            .ok_or(Failure::Missing(self.name))
    }
}
impl Changeable for Reservations {
    type Yield = Vec<Reservation>;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Vec<Reservation>, Failure> {
        m.records()
    }
}
impl Changeable for Deposit {
    type Yield = Deposited;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Deposited, Failure> {
        if self.amount < 0 {
            return Err(Failure::InvalidAmount);
        }
        let mut a = m
            .recall::<Account>(&self.name)?
            .ok_or(Failure::Missing(self.name))?;
        a.balance = a
            .balance
            .checked_add(self.amount)
            .ok_or(Failure::Overflow)?;
        m.remember(a.clone())?;
        m.changed.push(a.clone());
        Ok(Deposited { value: a.balance })
    }
}
impl Changeable for Withdraw {
    type Yield = Withdrawn;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Withdrawn, Failure> {
        if self.amount < 0 {
            return Err(Failure::InvalidAmount);
        }
        let mut a = m
            .recall::<Account>(&self.name)?
            .ok_or(Failure::Missing(self.name))?;
        if a.balance < self.amount {
            return Err(Failure::Insufficient(a.balance));
        }
        a.balance -= self.amount;
        m.remember(a.clone())?;
        m.changed.push(a.clone());
        Ok(Withdrawn { value: a.balance })
    }
}
impl Changeable for Transfer {
    type Yield = Transferred;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Transferred, Failure> {
        m.run(Withdraw {
            name: self.from,
            amount: self.amount,
        })?;
        m.run(Deposit {
            name: self.to,
            amount: self.amount,
        })?;
        Ok(Transferred {})
    }
}
impl Changeable for Reserve {
    type Yield = Reserved;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Reserved, Failure> {
        if m.recall::<Reservation>(&self.reservation_id)?.is_some() {
            return Err(Failure::Duplicate(self.reservation_id));
        }
        m.run(Withdraw {
            name: self.name.clone(),
            amount: self.amount,
        })?;
        m.remember(Reservation {
            reservation_id: self.reservation_id,
            name: self.name,
            amount: self.amount,
        })?;
        Ok(Reserved {})
    }
}
impl Changeable for Settle {
    type Yield = Settled;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Settled, Failure> {
        if m.recall::<Reservation>(&self.reservation_id)?.is_none() {
            return Err(Failure::UnknownReservation(self.reservation_id));
        }
        m.forget::<Reservation>(&self.reservation_id)?;
        Ok(Settled {})
    }
}
impl Changeable for Release {
    type Yield = Released;
    type Refusal = Failure;
    fn change(self, m: &mut Transaction) -> Result<Released, Failure> {
        let r = m
            .recall::<Reservation>(&self.reservation_id)?
            .ok_or(Failure::UnknownReservation(self.reservation_id.clone()))?;
        m.run(Deposit {
            name: r.name,
            amount: r.amount,
        })?;
        m.forget::<Reservation>(&self.reservation_id)?;
        Ok(Released {})
    }
}
impl Operable for Pay {
    type Outcome = PayOutcome;
    async fn operate(self, s: &Scope) -> PayOutcome {
        let r = Reservation {
            reservation_id: self.reservation_id,
            name: self.name,
            amount: self.amount,
        };
        if let Err(e) = s
            .run(Reserve {
                reservation_id: r.reservation_id.clone(),
                name: r.name.clone(),
                amount: r.amount,
            })
            .await
        {
            return PayOutcome::Refused(e);
        }
        match s
            .remit(Remit {
                reservation: r.clone(),
            })
            .await
        {
            RemitOutcome::Remitted => match s
                .run(Settle {
                    reservation_id: r.reservation_id,
                })
                .await
            {
                Ok(_) => PayOutcome::Paid,
                Err(e) => PayOutcome::Refused(e),
            },
            RemitOutcome::Unremitted(code) => match s
                .run(Release {
                    reservation_id: r.reservation_id,
                })
                .await
            {
                Ok(_) => PayOutcome::Unpaid(code),
                Err(e) => PayOutcome::Refused(e),
            },
        }
    }
}
impl Operable for Resume {
    type Outcome = Result<Resumed, Failure>;
    async fn operate(self, s: &Scope) -> Self::Outcome {
        let reservations = s.run(Reservations {}).await?;
        let mut n = 0;
        for r in reservations {
            match s
                .status(Status {
                    reservation_id: r.reservation_id.clone(),
                })
                .await
            {
                RemitOutcome::Remitted => {
                    s.run(Settle {
                        reservation_id: r.reservation_id,
                    })
                    .await?;
                }
                RemitOutcome::Unremitted(_) => {
                    s.run(Release {
                        reservation_id: r.reservation_id,
                    })
                    .await?;
                }
            }
            n += 1;
        }
        Ok(Resumed { value: n })
    }
}
impl Operable for TimerFired {
    type Outcome = Result<Deposited, Failure>;
    async fn operate(self, s: &Scope) -> Self::Outcome {
        s.run(Deposit {
            name: self.name,
            amount: self.amount,
        })
        .await
    }
}
