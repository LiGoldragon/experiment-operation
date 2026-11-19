use super::{ledger::*, *};

struct Fixture {
    directory: tempfile::TempDir,
    scope: Scope,
}
trait Prepared {
    async fn prepare(variant: Variant) -> Self;
    async fn balance(&self, name: &str) -> i64;
}
impl Prepared for Fixture {
    async fn prepare(variant: Variant) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let scope = Scope::open(&directory.path().join("ledger.redb"), variant, 16).unwrap();
        for name in ["alice", "bob"] {
            scope
                .run(Remember(Account {
                    name: name.into(),
                    balance: 12,
                }))
                .await
                .unwrap();
        }
        Self { directory, scope }
    }
    async fn balance(&self, name: &str) -> i64 {
        self.scope
            .run(Recall::<Account> {
                key: name.into(),
                record: PhantomData,
            })
            .await
            .unwrap()
            .unwrap()
            .balance
    }
}
#[tokio::test]
async fn both_backends_transfer_and_persist() {
    for variant in [Variant::Tasks, Variant::Actors] {
        let fixture = Fixture::prepare(variant).await;
        let root = fixture.scope.clone();
        tokio::spawn(async move {
            root.run(Transfer {
                from: "alice".into(),
                to: "bob".into(),
                amount: 5,
            })
            .await
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(fixture.balance("alice").await, 7);
        assert_eq!(fixture.balance("bob").await, 17);
        fixture.scope.shutdown().await;
        let reopened =
            Scope::open(&fixture.directory.path().join("ledger.redb"), variant, 16).unwrap();
        let account = reopened
            .run(Recall::<Account> {
                key: "alice".into(),
                record: PhantomData,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.balance, 7);
        reopened.shutdown().await;
    }
}
#[tokio::test]
async fn failed_suboperation_leaves_previous_commit() {
    for variant in [Variant::Tasks, Variant::Actors] {
        let fixture = Fixture::prepare(variant).await;
        let result = fixture
            .scope
            .run(Transfer {
                from: "alice".into(),
                to: "missing".into(),
                amount: 5,
            })
            .await;
        assert_eq!(result, Err(Failure::Missing("missing".into())));
        assert_eq!(fixture.balance("alice").await, 7);
        assert_eq!(fixture.balance("bob").await, 12);
        assert_eq!(
            fixture
                .scope
                .run(Withdraw {
                    name: "alice".into(),
                    amount: 8
                })
                .await,
            Err(Failure::Insufficient(7))
        );
        fixture.scope.shutdown().await;
    }
}
#[tokio::test]
async fn atomic_rollback_forget_and_validation() {
    for variant in [Variant::Tasks, Variant::Actors] {
        let fixture = Fixture::prepare(variant).await;
        let changed = Account {
            name: "alice".into(),
            balance: 99,
        };
        let result = fixture
            .scope
            .run(Atomic(vec![
                Change::Remember {
                    table: Account::TABLE,
                    key: changed.identity(),
                    bytes: changed.encode().unwrap(),
                },
                Change::Reject,
            ]))
            .await;
        assert_eq!(result, Err(Failure::Rejected));
        assert_eq!(fixture.balance("alice").await, 12);
        fixture
            .scope
            .run(Forget::<Account> {
                key: "bob".into(),
                record: PhantomData,
            })
            .await
            .unwrap();
        assert!(
            fixture
                .scope
                .run(Recall::<Account> {
                    key: "bob".into(),
                    record: PhantomData
                })
                .await
                .unwrap()
                .is_none()
        );
        assert!(Account::decode(&[1, 2, 3]).is_err());
        assert_eq!(
            fixture
                .scope
                .run(Deposit {
                    name: "alice".into(),
                    amount: -1
                })
                .await,
            Err(Failure::InvalidAmount)
        );
        fixture.scope.shutdown().await;
    }
}
struct Paused {
    entered: tokio::sync::oneshot::Sender<()>,
    resume: tokio::sync::oneshot::Receiver<()>,
}
impl Textualizable for Paused {
    fn datom(&self) -> String {
        "Paused".into()
    }
}
impl Operable for Paused {
    type Outcome = Result<(), Failure>;
    async fn operate(self, scope: &Scope) -> Self::Outcome {
        scope
            .run(Withdraw {
                name: "alice".into(),
                amount: 5,
            })
            .await?;
        let _ = self.entered.send(());
        let _ = self.resume.await;
        scope
            .run(Deposit {
                name: "bob".into(),
                amount: 5,
            })
            .await?;
        Ok(())
    }
}
#[tokio::test]
async fn cancellation_between_changes_is_not_replayed() {
    for variant in [Variant::Tasks, Variant::Actors] {
        let fixture = Fixture::prepare(variant).await;
        let (entered, observed) = oneshot::channel();
        let (_release, resume) = oneshot::channel();
        let root = fixture.scope.clone();
        let task = tokio::spawn(async move { root.run(Paused { entered, resume }).await });
        observed.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        fixture.scope.shutdown().await;
        let scope =
            Scope::open(&fixture.directory.path().join("ledger.redb"), variant, 16).unwrap();
        let account = scope
            .run(Recall::<Account> {
                key: "alice".into(),
                record: PhantomData,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.balance, 7);
        let account = scope
            .run(Recall::<Account> {
                key: "bob".into(),
                record: PhantomData,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.balance, 12);
        scope.shutdown().await;
    }
}
#[cfg(feature = "trace")]
#[tokio::test]
async fn whole_trace_matches_for_both_backends() {
    let mut traces = Vec::new();
    for variant in [Variant::Tasks, Variant::Actors] {
        let fixture = Fixture::prepare(variant).await;
        let offset = fixture.scope.trace().len();
        fixture
            .scope
            .run(Transfer {
                from: "alice".into(),
                to: "bob".into(),
                amount: 5,
            })
            .await
            .unwrap();
        let trace = fixture.scope.trace()[offset..].to_vec();
        assert_eq!(trace.len(), 18);
        assert_eq!(trace[0], "{ [  ] Entered.Transfer.{ «alice» «bob» 5 } }");
        assert_eq!(trace[1], "{ [ 0 ] Entered.Withdraw.{ «alice» 5 } }");
        assert_eq!(trace[2], "{ [ 0 0 ] Entered.Recall.«alice» }");
        assert_eq!(trace.last().unwrap(), "{ [  ] Yielded.Accepted.Succeeded }");
        traces.push(trace);
        fixture.scope.shutdown().await;
    }
    assert_eq!(traces[0], traces[1]);
}
