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
impl Changeable<NexusDomain> for Transfer {
    type Yield = Transferred;
    fn change(self, m: &mut Memory) -> Result<Transferred, Refusal> {
        m.run(Withdraw {
            name: Name(self.from.0),
            amount: self.amount,
        })?;
        m.run(Deposit {
            name: Name(self.to.0),
            amount: self.amount,
        })?;
        Ok(Transferred {})
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
impl Operable<NexusDomain> for Pay {
    type Outcome = Result<PayOutcome, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        let path = s.configuration.lock().unwrap().lock_socket.clone();
        let result: trial_lock::Output = s
            .ask(
                &path,
                trial_lock::SIGNAL_FINGERPRINT,
                trial_lock::Input::Lock(trial_lock::Lock {
                    lock_name: trial_lock::LockName(self.reservation.name.0.clone()),
                }),
            )
            .await?;
        if !matches!(result, trial_lock::Output::Locked(_)) {
            return Err(Refused::Invalid.into());
        }
        let reservation = self.reservation;
        let reserved = s
            .run(Reserve {
                reservation: reservation.clone(),
            })
            .await;
        let result = match reserved {
            Ok(_) => {
                s.boundary(Boundary::Reserved).await?;
                reservation.clone().finish(s, false).await
            }
            Err(e) => Err(e),
        };
        let released: Result<trial_lock::Output, Failure> = s
            .ask(
                &path,
                trial_lock::SIGNAL_FINGERPRINT,
                trial_lock::Input::Release(trial_lock::Release {
                    lock_name: trial_lock::LockName(reservation.name.0),
                }),
            )
            .await;
        released?;
        result
    }
}
// The shared settlement is a trait on the durable record, used by Pay and startup.
trait Finishing {
    fn finish(
        self,
        s: &Runtime,
        status: bool,
    ) -> impl std::future::Future<Output = Result<PayOutcome, Refusal>> + Send;
}
impl Finishing for Reservation {
    async fn finish(self, s: &Runtime, status: bool) -> Result<PayOutcome, Refusal> {
        let path = s.configuration.lock().unwrap().translator_socket.clone();
        let outcome = if status {
            s.ask::<_, RemitOutcome>(
                &path,
                SIGNAL_FINGERPRINT,
                WorldInput::Status(Status {
                    reservation_id: self.reservation_id.clone(),
                }),
            )
            .await?
        } else {
            RemitOutcome::Unknown
        };
        let outcome = match outcome {
            RemitOutcome::Unknown => {
                s.boundary(Boundary::BeforeCall).await?;
                s.ask::<_, RemitOutcome>(
                    &path,
                    SIGNAL_FINGERPRINT,
                    WorldInput::Remit(Remit {
                        reservation: self.clone(),
                    }),
                )
                .await?
            }
            outcome => outcome,
        };
        s.boundary(Boundary::AfterReply).await?;
        match outcome {
            RemitOutcome::Remitted => {
                s.boundary(Boundary::BeforeSettle).await?;
                s.run(Settle {
                    reservation_id: self.reservation_id,
                })
                .await?;
                s.boundary(Boundary::AfterSettle).await?;
                Ok(PayOutcome::Paid)
            }
            RemitOutcome::Unremitted(code) => {
                s.run(Release {
                    reservation_id: self.reservation_id,
                })
                .await?;
                Ok(PayOutcome::Unpaid(code))
            }
            RemitOutcome::Unknown => Err(Refused::Invalid.into()),
        }
    }
}
impl Operable<NexusDomain> for Reservation {
    type Outcome = Result<PayOutcome, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        let path = s.configuration.lock().unwrap().lock_socket.clone();
        let name = self.name.0.clone();
        let result = self.finish(s, true).await;
        if result.is_ok() {
            let _: trial_lock::Output = s
                .ask(
                    &path,
                    trial_lock::SIGNAL_FINGERPRINT,
                    trial_lock::Input::Release(trial_lock::Release {
                        lock_name: trial_lock::LockName(name),
                    }),
                )
                .await?;
        }
        result
    }
}
impl Operable<NexusDomain> for Subscribe {
    type Outcome = Result<Subscription<Account>, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        Ok(s.subscribe::<Account>(self.name.encode()?).await?)
    }
}

impl Operable<NexusDomain> for Payment {
    type Outcome = Result<PayOutcome, Refusal>;
    async fn operate(self, s: &Runtime) -> Self::Outcome {
        let path = s.configuration.lock().unwrap().lock_socket.clone();
        let output: trial_lock::Output = s
            .ask(
                &path,
                trial_lock::SIGNAL_FINGERPRINT,
                trial_lock::Input::Release(trial_lock::Release {
                    lock_name: trial_lock::LockName(self.name.0),
                }),
            )
            .await?;
        match output {
            trial_lock::Output::Released(_)
            | trial_lock::Output::Refused(trial_lock::Refused::Unknown) => Ok(if self.boolean {
                PayOutcome::Paid
            } else {
                PayOutcome::Unpaid(1)
            }),
            _ => Err(Refused::Invalid.into()),
        }
    }
}
