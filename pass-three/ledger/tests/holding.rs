use operation_scope::*;
use trial_ledger::*;
fn configuration() -> Configuration {
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
struct Take(Account);
impl TraceObject<NexusDomain> for Take {
    fn object(&self) -> Object {
        Object::Create(Create {
            account: self.0.clone(),
        })
    }
}
impl Changeable<NexusDomain> for Take {
    type Yield = Account;
    fn change(self, m: &mut Transaction<NexusDomain>) -> Result<Account, Refusal> {
        if m.recall::<Account>(&self.0.name.encode()?)?.is_some() {
            return Err(Refused::Invalid.into());
        }
        m.remember_held(self.0.clone())?;
        Ok(self.0)
    }
}
fn account() -> Account {
    Account {
        name: Name("held".into()),
        balance: Balance(0),
    }
}
#[tokio::test]
async fn held_record_ends_with_its_holder_and_cancelled_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let scope = Scope::<NexusDomain>::open(
        dir.path().join("memory"),
        configuration(),
        TraceHub::open(8192),
    )
    .await
    .unwrap();
    let mut subscription = scope
        .subscribe::<Account>(account().name.encode().unwrap())
        .await
        .unwrap();
    let held = scope.hold(Take(account())).await.unwrap();
    assert_eq!(subscription.changed().await.unwrap(), Some(account()));
    assert!(scope.hold(Take(account())).await.is_err());
    drop(held);
    assert_eq!(subscription.changed().await.unwrap(), None);
    let (ready, wait) = tokio::sync::oneshot::channel();
    let child = scope.clone();
    let task = tokio::spawn(async move {
        let _held = child.hold(Take(account())).await.unwrap();
        ready.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    wait.await.unwrap();
    assert_eq!(subscription.changed().await.unwrap(), Some(account()));
    task.abort();
    let _ = task.await;
    assert_eq!(subscription.changed().await.unwrap(), None);
    let _held = scope.hold(Take(account())).await.unwrap();
}
#[tokio::test]
async fn persisted_hold_is_removed_before_startup_returns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory");
    let scope = Scope::<NexusDomain>::open(path.clone(), configuration(), TraceHub::open(8192))
        .await
        .unwrap();
    let held = scope.hold(Take(account())).await.unwrap();
    // Model death without destructors: retain the durable record while closing the owner.
    std::mem::forget(held);
    scope.shutdown().await;
    let reopened = Scope::<NexusDomain>::open(path, configuration(), TraceHub::open(8192))
        .await
        .unwrap();
    assert!(
        reopened
            .run(Inspect {
                name: account().name
            })
            .await
            .is_err()
    );
    assert!(reopened.hold(Take(account())).await.is_ok());
}

struct InterruptedTake {
    entered: tokio::sync::oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
impl TraceObject<NexusDomain> for InterruptedTake {
    fn object(&self) -> Object {
        Object::Create(Create { account: account() })
    }
}
impl Changeable<NexusDomain> for InterruptedTake {
    type Yield = Account;
    fn change(self, m: &mut Transaction<NexusDomain>) -> Result<Account, Refusal> {
        m.remember_held(account())?;
        self.entered.send(()).unwrap();
        self.release.recv().unwrap();
        Ok(account())
    }
}
#[tokio::test]
async fn cancellation_before_acquisition_reply_has_no_ownership_gap() {
    let dir = tempfile::tempdir().unwrap();
    let scope = Scope::<NexusDomain>::open(
        dir.path().join("memory"),
        configuration(),
        TraceHub::open(8192),
    )
    .await
    .unwrap();
    let (entered, wait) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let child = scope.clone();
    let task = tokio::spawn(async move {
        child
            .hold(InterruptedTake {
                entered,
                release: blocked,
            })
            .await
    });
    wait.await.unwrap();
    task.abort();
    assert!(task.await.is_err());
    release.send(()).unwrap();
    // Owner FIFO places this inspection after the acquisition and its queued cleanup.
    assert!(
        scope
            .run(Inspect {
                name: account().name
            })
            .await
            .is_err()
    );
    assert!(scope.hold(Take(account())).await.is_ok());
}
