use crate::*;
use operation_scope::*;
impl Changeable<NexusDomain> for Lock {
    type Yield = Locked;
    fn change(self, m: &mut Transaction<NexusDomain>) -> Result<Locked, Refusal> {
        let key = self.lock_name.encode()?;
        if m.recall::<HeldLock>(&key)?.is_some() {
            return Err(Refused::Held.into());
        }
        m.remember(HeldLock {
            lock_name: self.lock_name,
            integer: 0,
        })?;
        Ok(Locked {})
    }
}
impl Changeable<NexusDomain> for Release {
    type Yield = Released;
    fn change(self, m: &mut Transaction<NexusDomain>) -> Result<Released, Refusal> {
        let key = self.lock_name.encode()?;
        if m.recall::<HeldLock>(&key)?.is_none() {
            return Err(Refused::Unknown.into());
        }
        m.forget::<HeldLock>(&key)?;
        Ok(Released {})
    }
}
