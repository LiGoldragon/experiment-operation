use crate::*;
use operation_scope::*;
impl Changeable<NexusDomain> for TakeLock {
    type Yield = Locked;
    fn change(self, m: &mut Transaction<NexusDomain>) -> Result<Locked, Refusal> {
        let key = self.lock_name.encode()?;
        if m.recall::<HeldLock>(&key)?.is_some() {
            return Err(Refused::Held.into());
        }
        m.remember_held(HeldLock {
            lock_name: self.lock_name,
            integer: 0,
        })?;
        Ok(Locked {})
    }
}

impl Operable<NexusDomain> for AwaitExchange {
    type Outcome = Result<Locked, Refusal>;
    async fn operate(self, scope: &Scope<NexusDomain>) -> Self::Outcome {
        scope.await_exchange().await?;
        Ok(Locked {})
    }
}
