use operation_scope::*;
use trial_lock::*;
const ORIGINAL: [u8; 32] = [
    154, 70, 12, 5, 29, 183, 154, 248, 167, 189, 212, 176, 211, 255, 203, 248, 5, 131, 130, 73,
    208, 134, 33, 191, 50, 69, 136, 253, 193, 213, 170, 150,
];
fn config() -> Configuration {
    Configuration {
        max_frame: 1048576,
        command_capacity: 64,
        trace_capacity: 8192,
        max_subscriptions: 16,
        lock_socket: String::new(),
        translator_socket: String::new(),
        observer_socket: String::new(),
        pause_at: None,
    }
}
#[derive(Clone)]
struct Earlier<const UNKNOWN: bool>;
impl<const UNKNOWN: bool> Domain for Earlier<UNKNOWN> {
    type Object = Object;
    type Outcome = Outcome;
    type Refusal = Refusal;
    const MEMORY_FINGERPRINT: [u8; 32] = if UNKNOWN { [9; 32] } else { ORIGINAL };
    const SIGNAL_FINGERPRINT: [u8; 32] = SIGNAL_FINGERPRINT;
}
impl<const U: bool> TraceOutcome<Earlier<U>> for Refusal {
    fn outcome(&self) -> Outcome {
        Outcome::Refusal(self.clone())
    }
}
impl<const U: bool> TraceOutcome<Earlier<U>> for Locked {
    fn outcome(&self) -> Outcome {
        Outcome::Locked(self.clone())
    }
}
struct Seed;
impl<const U: bool> TraceObject<Earlier<U>> for Seed {
    fn object(&self) -> Object {
        Object::Lock(Lock {
            lock_name: LockName("seed".into()),
        })
    }
}
impl<const U: bool> Changeable<Earlier<U>> for Seed {
    type Yield = Locked;
    fn change(self, m: &mut Transaction<Earlier<U>>) -> Result<Locked, Refusal> {
        m.remember(history::v0::HeldLock {
            lock_name: history::v0::LockName("seed".into()),
        })?;
        Ok(Locked {})
    }
}
struct Inspect;
impl TraceObject<NexusDomain> for Inspect {
    fn object(&self) -> Object {
        Object::Lock(Lock {
            lock_name: LockName("seed".into()),
        })
    }
}
impl Changeable<NexusDomain> for Inspect {
    type Yield = HeldLock;
    fn change(self, m: &mut Transaction<NexusDomain>) -> Result<HeldLock, Refusal> {
        m.recall::<HeldLock>(&LockName("seed".into()).encode()?)?
            .ok_or(Refused::Unknown.into())
    }
}
#[tokio::test]
async fn first_change_upgrades_actual_old_record_and_stamp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("memory");
    let old = Scope::<Earlier<false>>::open(path.clone(), config(), TraceHub::open(8192))
        .await
        .unwrap();
    old.change(Seed).await.unwrap();
    old.shutdown().await;
    let traces = TraceHub::<NexusDomain>::open(8192);
    let mut receiver = traces.sender.subscribe();
    let current = Scope::<NexusDomain>::open(path.clone(), config(), traces)
        .await
        .unwrap();
    assert_eq!(current.change(Inspect).await.unwrap().integer, 0);
    #[cfg(feature = "trace")]
    {
        let first = receiver.try_recv().unwrap();
        let second = receiver.try_recv().unwrap();
        assert_eq!(first.path, Vec::<u64>::new());
        assert!(matches!(
            first.event,
            operation_scope::Event::Entered(Object::Upgrade(_))
        ));
        assert!(matches!(
            second.event,
            operation_scope::Event::Yielded(Outcome::UpgradeYield(UpgradeYield { records: 1, .. }))
        ));
    }
    #[cfg(not(feature = "trace"))]
    let _ = &mut receiver;
    current.shutdown().await;
    let reopened = Scope::<NexusDomain>::open(path, config(), TraceHub::open(8192))
        .await
        .unwrap();
    assert_eq!(reopened.change(Inspect).await.unwrap().integer, 0);
    reopened.shutdown().await;
}
#[tokio::test]
async fn unknown_memory_stamp_is_refused_before_decoding_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("memory");
    let unknown = Scope::<Earlier<true>>::open(path.clone(), config(), TraceHub::open(8192))
        .await
        .unwrap();
    unknown.change(Seed).await.unwrap();
    unknown.shutdown().await;
    let result = Scope::<NexusDomain>::open(path, config(), TraceHub::open(8192)).await;
    assert!(
        matches!(result,Err(Failure::Schema{stored,expected}) if stored==[9;32] && expected==MEMORY_FINGERPRINT)
    );
}
