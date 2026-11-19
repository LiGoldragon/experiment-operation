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
async fn scope() -> (tempfile::TempDir, Scope<NexusDomain>) {
    let dir = tempfile::tempdir().unwrap();
    let traces = TraceHub::open(8192);
    let scope = Scope::open(dir.path().join("ledger"), configuration(), traces)
        .await
        .unwrap();
    (dir, scope)
}
async fn seed(scope: &Scope<NexusDomain>) {
    scope
        .run(Create {
            account: Account {
                name: Name("alice".into()),
                balance: Balance(12),
            },
        })
        .await
        .unwrap();
    scope
        .run(Create {
            account: Account {
                name: Name("bob".into()),
                balance: Balance(4),
            },
        })
        .await
        .unwrap();
}
#[tokio::test]
async fn changes_are_whole_and_serialized() {
    let (_dir, scope) = scope().await;
    seed(&scope).await;
    assert!(
        scope
            .run(Transfer {
                from: From("alice".into()),
                to: To("missing".into()),
                amount: Amount(5)
            })
            .await
            .is_err()
    );
    assert_eq!(
        scope
            .run(Inspect {
                name: Name("alice".into())
            })
            .await
            .unwrap()
            .balance
            .0,
        12
    );
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let scope = scope.clone();
        tasks.spawn(async move {
            scope
                .run(Deposit {
                    name: Name("alice".into()),
                    amount: Amount(1),
                })
                .await
                .unwrap();
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(
        scope
            .run(Inspect {
                name: Name("alice".into())
            })
            .await
            .unwrap()
            .balance
            .0,
        44
    );
}
#[tokio::test]
async fn subscriber_keeps_one_latest_state_for_thousand_changes() {
    let (_dir, scope) = scope().await;
    seed(&scope).await;
    let mut subscription = scope
        .run(Subscribe {
            name: Name("alice".into()),
        })
        .await
        .unwrap();
    assert_eq!(subscription.latest().unwrap().unwrap().balance.0, 12);
    for _ in 0..1000 {
        scope
            .run(Deposit {
                name: Name("alice".into()),
                amount: Amount(1),
            })
            .await
            .unwrap();
    }
    assert_eq!(subscription.latest().unwrap().unwrap().balance.0, 1012);
    assert!(!subscription.receiver.has_changed().unwrap());
    // Tokio watch retains one value, not a change history; receiver sees final bytes once.
    assert_eq!(
        subscription.receiver.borrow().as_ref().unwrap().len(),
        Account {
            name: Name("alice".into()),
            balance: Balance(1012)
        }
        .encode()
        .unwrap()
        .len()
    );
}
#[tokio::test]
async fn refused_nested_change_poison_is_sticky() {
    // Define an authored body that catches the refusal deliberately.
    #[derive(Clone)]
    struct Catch;
    impl TraceObject<NexusDomain> for Catch {
        fn object(&self) -> Object {
            Object::Transfer(Transfer {
                from: From("alice".into()),
                to: To("missing".into()),
                amount: Amount(5),
            })
        }
    }
    impl Changeable<NexusDomain> for Catch {
        type Yield = Transferred;
        fn change(self, m: &mut Transaction<NexusDomain>) -> Result<Transferred, Refusal> {
            let _ = m.run(Transfer {
                from: From("alice".into()),
                to: To("missing".into()),
                amount: Amount(5),
            });
            m.remember(Account {
                name: Name("alice".into()),
                balance: Balance(99),
            })?;
            Ok(Transferred {})
        }
    }
    let (_dir, scope) = scope().await;
    seed(&scope).await;
    assert!(scope.change(Catch).await.is_err());
    assert_eq!(
        scope
            .run(Inspect {
                name: Name("alice".into())
            })
            .await
            .unwrap()
            .balance
            .0,
        12
    );
}
#[cfg(feature = "trace")]
#[tokio::test]
async fn root_yield_is_after_commit_and_failed_commit_agrees() {
    let (_dir, scope) = scope().await;
    seed(&scope).await;
    let mut traces = scope.traces.sender.subscribe();
    scope
        .fail_commit
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let result = scope
        .run(Transfer {
            from: From("alice".into()),
            to: To("bob".into()),
            amount: Amount(5),
        })
        .await;
    assert_eq!(result, Err(Refusal::Runtime(Failure::Unstored)));
    let mut actual = Vec::new();
    while let Ok(trace) = traces.try_recv() {
        actual.push(trace);
    }
    assert_eq!(actual.len(), 6);
    assert!(matches!(
        &actual[0].event,
        operation_scope::Event::Entered(Object::Transfer(_))
    ));
    assert_eq!(actual[1].path, vec![0]);
    assert_eq!(actual[3].path, vec![1]);
    assert!(matches!(
        &actual[5].event,
        operation_scope::Event::Yielded(Outcome::Refusal(Refusal::Runtime(Failure::Unstored)))
    ));
    assert_eq!(
        scope
            .run(Inspect {
                name: Name("alice".into())
            })
            .await
            .unwrap()
            .balance
            .0,
        12
    );
    assert_eq!(
        scope
            .run(Inspect {
                name: Name("bob".into())
            })
            .await
            .unwrap()
            .balance
            .0,
        4
    );
}
