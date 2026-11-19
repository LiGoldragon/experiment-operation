use crate::*;
use operation_scope::*;
type Memory = Transaction<NexusDomain>;
type Runtime = Scope<NexusDomain>;
impl Changeable<NexusDomain> for Create {
    type Yield = Created;
    fn change(self, m: &mut Memory) -> Result<Created, Refusal> {
        if m.recall::<Account>(&self.account.name.encode()?)?.is_some() {
            return Err(Refused::Invalid.into());
        }
        if self.account.balance.0 < 0 {
            return Err(Refused::Invalid.into());
        }
        m.remember(self.account)?;
        Ok(Created {})
    }
}
impl Changeable<NexusDomain> for Inspect {
    type Yield = Account;
    fn change(self, m: &mut Memory) -> Result<Account, Refusal> {
        m.recall::<Account>(&self.name.encode()?)?
            .ok_or_else(|| Refused::Missing(self.name).into())
    }
}
impl Changeable<NexusDomain> for Deposit {
    type Yield = Deposited;
    fn change(self, m: &mut Memory) -> Result<Deposited, Refusal> {
        if self.amount.0 < 0 {
            return Err(Refused::Invalid.into());
        }
        let mut account = m
            .recall::<Account>(&self.name.encode()?)?
            .ok_or(Refused::Missing(self.name))?;
        account.balance.0 = account
            .balance
            .0
            .checked_add(self.amount.0)
            .ok_or(Refused::Invalid)?;
        m.remember(account.clone())?;
        Ok(Deposited {
            balance: account.balance,
        })
    }
}
impl Changeable<NexusDomain> for Withdraw {
    type Yield = Withdrawn;
    fn change(self, m: &mut Memory) -> Result<Withdrawn, Refusal> {
        if self.amount.0 < 0 {
            return Err(Refused::Invalid.into());
        }
        let mut account = m
            .recall::<Account>(&self.name.encode()?)?
            .ok_or(Refused::Missing(self.name))?;
        if account.balance.0 < self.amount.0 {
            return Err(Refused::Insufficient(account.balance).into());
        }
        account.balance.0 -= self.amount.0;
        m.remember(account.clone())?;
        Ok(Withdrawn {
            balance: account.balance,
        })
    }
}
impl Changeable<NexusDomain> for Reserve {
    type Yield = Reserved;
    fn change(self, m: &mut Memory) -> Result<Reserved, Refusal> {
        let key = self.reservation.reservation_id.encode()?;
        if m.recall::<Reservation>(&key)?.is_some() || m.recall::<Payment>(&key)?.is_some() {
            return Err(Refused::Duplicate(self.reservation.reservation_id).into());
        }
        m.run(Withdraw {
            name: self.reservation.name.clone(),
            amount: self.reservation.amount,
        })?;
        m.remember(self.reservation)?;
        Ok(Reserved {})
    }
}
impl Changeable<NexusDomain> for Settle {
    type Yield = Settled;
    fn change(self, m: &mut Memory) -> Result<Settled, Refusal> {
        let key = self.reservation_id.encode()?;
        let reservation = m.recall::<Reservation>(&key)?.ok_or(Refused::Invalid)?;
        m.remember(Payment {
            reservation_id: reservation.reservation_id,
            name: reservation.name,
            amount: reservation.amount,
            boolean: true,
        })?;
        m.forget::<Reservation>(&key)?;
        Ok(Settled {})
    }
}
impl Changeable<NexusDomain> for Release {
    type Yield = Released;
    fn change(self, m: &mut Memory) -> Result<Released, Refusal> {
        let key = self.reservation_id.encode()?;
        let reservation = m.recall::<Reservation>(&key)?.ok_or(Refused::Invalid)?;
        m.run(Deposit {
            name: reservation.name.clone(),
            amount: reservation.amount,
        })?;
        m.remember(Payment {
            reservation_id: reservation.reservation_id,
            name: reservation.name,
            amount: reservation.amount,
            boolean: false,
        })?;
        m.forget::<Reservation>(&key)?;
        Ok(Released {})
    }
}
impl Operable<NexusDomain> for AcquireLock {
    type Outcome = Result<Held<LockAcquired>, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        s.boundary(Boundary::BeforeLock).await?;
        let path = s.configuration.lock().unwrap().lock_socket.clone();
        let held = s
            .exchange::<_, trial_lock::Output>(
                &path,
                trial_lock::SIGNAL_FINGERPRINT,
                trial_lock::Input::Lock(trial_lock::Lock {
                    lock_name: trial_lock::LockName(self.reservation.name.0),
                }),
            )
            .await?;
        if !matches!(&held.value, trial_lock::Output::Locked(_)) {
            return Err(Refused::Invalid.into());
        }
        s.boundary(Boundary::AfterLock).await?;
        Ok(held.map(LockAcquired {}))
    }
}
impl Operable<NexusDomain> for BeforeReserve {
    type Outcome = Result<BoundaryReached, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        s.boundary(Boundary::BeforeReserve).await?;
        Ok(BoundaryReached {})
    }
}
impl Operable<NexusDomain> for ReservedBoundary {
    type Outcome = Result<BoundaryReached, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        s.boundary(Boundary::Reserved).await?;
        Ok(BoundaryReached {})
    }
}
impl Operable<NexusDomain> for BeforeSettle {
    type Outcome = Result<BoundaryReached, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        s.boundary(Boundary::BeforeSettle).await?;
        Ok(BoundaryReached {})
    }
}
impl Operable<NexusDomain> for AfterSettle {
    type Outcome = Result<BoundaryReached, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        s.boundary(Boundary::AfterSettle).await?;
        Ok(BoundaryReached {})
    }
}
impl Operable<NexusDomain> for Remitting {
    type Outcome = Result<RemitOutcome, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        let path = s.configuration.lock().unwrap().translator_socket.clone();
        s.boundary(Boundary::BeforeCall).await?;
        let outcome = s
            .ask::<_, RemitOutcome>(
                &path,
                SIGNAL_FINGERPRINT,
                WorldInput::Remit(Remit {
                    reservation: self.reservation,
                }),
            )
            .await?;
        s.boundary(Boundary::AfterReply).await?;
        Ok(outcome)
    }
}
impl Operable<NexusDomain> for Observing {
    type Outcome = Result<RemitOutcome, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        let path = s.configuration.lock().unwrap().translator_socket.clone();
        Ok(s.ask::<_, RemitOutcome>(
            &path,
            SIGNAL_FINGERPRINT,
            WorldInput::Status(Status {
                reservation_id: self.reservation.reservation_id,
            }),
        )
        .await?)
    }
}
impl Operable<NexusDomain> for Subscribe {
    type Outcome = Result<Subscription<Account>, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        Ok(s.subscribe::<Account>(self.name.encode()?).await?)
    }
}

impl Operable<NexusDomain> for Tail {
    type Outcome = Result<TailDone, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        s.boundary(Boundary::AfterOutput).await?;
        Ok(TailDone {})
    }
}
