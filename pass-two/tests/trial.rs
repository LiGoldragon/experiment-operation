use operation_ledger::{
    Runnable,
    ledger::*,
    scope::*,
    transport::{HolderMode, SubscriptionReceiving},
};
use std::time::Duration;

trait Fixture {
    fn open(
        &self,
        mode: HolderMode,
        translator: StubTranslator,
    ) -> impl std::future::Future<Output = Scope>;
}
impl Fixture for tempfile::TempDir {
    async fn open(&self, mode: HolderMode, translator: StubTranslator) -> Scope {
        let s = Scope::open(self.path().join("store"), mode, translator)
            .await
            .unwrap();
        s.run(Create {
            account: Account {
                name: "alice".into(),
                balance: 12,
            },
        })
        .await
        .unwrap();
        s.run(Create {
            account: Account {
                name: "bob".into(),
                balance: 4,
            },
        })
        .await
        .unwrap();
        s
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transfer_is_whole_and_roots_do_not_lose_updates() {
    for mode in [HolderMode::Task, HolderMode::Actor] {
        let dir = tempfile::tempdir().unwrap();
        let s = dir.open(mode, StubTranslator::default()).await;
        assert_eq!(
            s.run(Transfer {
                from: "alice".into(),
                to: "missing".into(),
                amount: 5
            })
            .await,
            Err(Failure::Missing("missing".into()))
        );
        assert_eq!(
            s.run(Inspect {
                name: "alice".into()
            })
            .await
            .unwrap()
            .balance,
            12
        );
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(33));
        let mut joins = Vec::new();
        for _ in 0..32 {
            let scope = s.clone();
            let barrier = barrier.clone();
            joins.push(tokio::spawn(async move {
                barrier.wait().await;
                scope
                    .run(Deposit {
                        name: "alice".into(),
                        amount: 1,
                    })
                    .await
                    .unwrap();
            }));
        }
        barrier.wait().await;
        for join in joins {
            join.await.unwrap();
        }
        assert_eq!(
            s.run(Inspect {
                name: "alice".into()
            })
            .await
            .unwrap()
            .balance,
            44
        );
        s.run(Transfer {
            from: "alice".into(),
            to: "bob".into(),
            amount: 5,
        })
        .await
        .unwrap();
        assert_eq!(
            s.run(Inspect {
                name: "alice".into()
            })
            .await
            .unwrap()
            .balance,
            39
        );
        assert_eq!(
            s.run(Inspect { name: "bob".into() }).await.unwrap().balance,
            9
        );
        assert_eq!(
            s.run(Deposit {
                name: "bob".into(),
                amount: -1
            })
            .await,
            Err(Failure::InvalidAmount)
        );
        assert_eq!(
            s.run(Withdraw {
                name: "bob".into(),
                amount: 10
            })
            .await,
            Err(Failure::Insufficient(9))
        );
        s.shutdown().await;
    }
}
#[tokio::test]
async fn pay_succeeds_fails_and_resumes_after_lost_reply() {
    for behavior in [
        RemitBehavior::Succeed,
        RemitBehavior::Fail(7),
        RemitBehavior::Never,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let translator = StubTranslator {
            behavior: behavior.clone(),
            ..Default::default()
        };
        let s = dir.open(HolderMode::Task, translator.clone()).await;
        let payment = Pay {
            reservation_id: "id".into(),
            name: "alice".into(),
            amount: 5,
        };
        match behavior {
            RemitBehavior::Succeed => assert_eq!(s.run(payment).await, PayOutcome::Paid),
            RemitBehavior::Fail(_) => assert_eq!(s.run(payment).await, PayOutcome::Unpaid(7)),
            RemitBehavior::Never => {
                let scope = s.clone();
                let task = tokio::spawn(async move { scope.run(payment).await });
                translator.reached.notified().await;
                assert_eq!(
                    s.run(Inspect {
                        name: "alice".into()
                    })
                    .await
                    .unwrap()
                    .balance,
                    7
                );
                assert_eq!(s.run(Reservations {}).await.unwrap().len(), 1);
                task.abort();
                let _ = task.await;
                #[cfg(feature = "trace")]
                assert!(
                    s.traces
                        .history
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|t| matches!(t.event, operation_ledger::trace::Event::Abandoned))
                );
                s.shutdown().await;
                let reopened = Scope::open(dir.path().join("store"), HolderMode::Task, translator)
                    .await
                    .unwrap();
                assert!(reopened.run(Reservations {}).await.unwrap().is_empty());
                assert_eq!(
                    reopened
                        .run(Inspect {
                            name: "alice".into()
                        })
                        .await
                        .unwrap()
                        .balance,
                    7
                );
                reopened.shutdown().await;
                continue;
            }
        }
        let expected = if matches!(behavior, RemitBehavior::Fail(_)) {
            12
        } else {
            7
        };
        assert_eq!(
            s.run(Inspect {
                name: "alice".into()
            })
            .await
            .unwrap()
            .balance,
            expected
        );
        assert!(s.run(Reservations {}).await.unwrap().is_empty());
        s.shutdown().await;
    }
}
#[tokio::test]
async fn subscription_is_ordered_filters_rollbacks_and_timer_enters_a_root() {
    for mode in [HolderMode::Task, HolderMode::Actor] {
        let dir = tempfile::tempdir().unwrap();
        let s = dir.open(mode, StubTranslator::default()).await;
        let mut sub = s.subscribe("alice".into()).await.unwrap();
        assert_eq!(sub.next().await.unwrap().balance, 12);
        s.run(Deposit {
            name: "bob".into(),
            amount: 1,
        })
        .await
        .unwrap();
        assert!(
            s.run(Transfer {
                from: "alice".into(),
                to: "missing".into(),
                amount: 2
            })
            .await
            .is_err()
        );
        s.timer(
            TimerFired {
                name: "alice".into(),
                amount: 3,
            },
            Duration::from_millis(1),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), sub.next())
                .await
                .unwrap()
                .unwrap()
                .balance,
            15
        );
        s.run(Transfer {
            from: "alice".into(),
            to: "alice".into(),
            amount: 2,
        })
        .await
        .unwrap();
        assert_eq!(sub.next().await.unwrap().balance, 15);
        drop(sub);
        s.shutdown().await;
        #[cfg(feature = "trace")]
        {
            use operation_ledger::trace::{Event, Object};
            let trace = s.traces.history.lock().unwrap();
            assert!(trace.iter().any(|t| matches!(t.event, Event::Ended)));
            assert!(
                trace
                    .iter()
                    .any(|t| matches!(t.event, Event::Entered(Object::TimerFired(_)))
                        && t.path.is_empty())
            );
        }
    }
}
#[cfg(feature = "trace")]
#[tokio::test]
async fn exact_transfer_trace_is_typed_and_unique_per_root() {
    use operation_ledger::trace::{Event, Object, Outcome, Trace};
    let dir = tempfile::tempdir().unwrap();
    let s = dir.open(HolderMode::Task, StubTranslator::default()).await;
    s.traces.history.lock().unwrap().clear();
    s.run(Transfer {
        from: "alice".into(),
        to: "bob".into(),
        amount: 5,
    })
    .await
    .unwrap();
    let traces = s.traces.history.lock().unwrap().clone();
    assert_eq!(traces.len(), 6);
    let root = traces[0].root;
    let expected = vec![
        Trace {
            root,
            path: vec![],
            event: Event::Entered(Object::Transfer(Transfer {
                from: "alice".into(),
                to: "bob".into(),
                amount: 5,
            })),
        },
        Trace {
            root,
            path: vec![0],
            event: Event::Entered(Object::Withdraw(Withdraw {
                name: "alice".into(),
                amount: 5,
            })),
        },
        Trace {
            root,
            path: vec![0],
            event: Event::Yielded(Outcome::Withdrawn(Withdrawn { value: 7 })),
        },
        Trace {
            root,
            path: vec![1],
            event: Event::Entered(Object::Deposit(Deposit {
                name: "bob".into(),
                amount: 5,
            })),
        },
        Trace {
            root,
            path: vec![1],
            event: Event::Yielded(Outcome::Deposited(Deposited { value: 9 })),
        },
        Trace {
            root,
            path: vec![],
            event: Event::Yielded(Outcome::Transferred(Transferred {})),
        },
    ];
    assert_eq!(traces, expected);
    s.run(Inspect {
        name: "alice".into(),
    })
    .await
    .unwrap();
    assert_ne!(s.traces.history.lock().unwrap().last().unwrap().root, root);
    s.shutdown().await;
}
#[cfg(feature = "trace")]
#[tokio::test]
async fn trace_socket_carries_checked_archives_and_debug_text_escapes_names() {
    use datom_codec::Datomizable;
    use operation_ledger::{
        trace::{Event, Object, Trace, TraceServing},
        transport::FrameCarrying,
    };
    use protos::{Protosizable, Textualizable};
    let dir = tempfile::tempdir().unwrap();
    let s = dir.open(HolderMode::Task, StubTranslator::default()).await;
    let socket = dir.path().join("trace.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let hub = s.traces.clone();
    let server = tokio::spawn(async move { hub.serve(listener).await });
    let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    assert!(stream.read_frame().await.unwrap().is_empty());
    s.run(Transfer {
        from: "alice".into(),
        to: "bob".into(),
        amount: 5,
    })
    .await
    .unwrap();
    let mut received = Vec::new();
    for _ in 0..6 {
        let bytes = stream.read_frame().await.unwrap();
        received.push(rkyv::from_bytes::<Trace, rkyv::rancor::Error>(&bytes).unwrap());
    }
    assert!(matches!(
        received[0].event,
        Event::Entered(Object::Transfer(_))
    ));
    assert!(received[5].path.is_empty());
    let text = received[0].datomize(Vec::new()).protosize().textualize();
    assert!(text.contains("Entered.Transfer"), "{text}");
    let odd = Trace {
        root: 91,
        path: vec![],
        event: Event::Entered(Object::Inspect(Inspect {
            name: "alice » { odd".into(),
        })),
    };
    let text = odd.datomize(Vec::new()).protosize().textualize();
    assert!(text.contains("alice"));
    assert!(text.contains("Inspect"));
    use experiment_upgrade::DatomReading;
    let inspect = Inspect {
        name: "alice » { odd".into(),
    };
    let text = inspect.datomize(Vec::new()).protosize().textualize();
    assert_eq!(text.as_str().read_datom::<Inspect>().unwrap(), inspect);
    server.abort();
    s.shutdown().await;
}
#[tokio::test]
async fn caught_child_refusal_still_poisoned_transaction() {
    use operation_ledger::{
        TransactionRunning,
        memory::{Store, StoreAccess, StoreOpening, TransactionAccess, TransactionCommit},
    };
    let dir = tempfile::tempdir().unwrap();
    let s = dir.open(HolderMode::Task, StubTranslator::default()).await;
    s.shutdown().await;
    let store = Store::open(dir.path().join("store")).unwrap();
    let mut tx = store.begin().unwrap();
    assert!(
        tx.run(Transfer {
            from: "alice".into(),
            to: "missing".into(),
            amount: 5
        })
        .is_err()
    );
    tx.remember(Account {
        name: "bob".into(),
        balance: 999,
    })
    .unwrap();
    assert!(tx.commit().is_err());
    let tx = store.begin().unwrap();
    assert_eq!(tx.recall::<Account>("alice").unwrap().unwrap().balance, 12);
    assert_eq!(tx.recall::<Account>("bob").unwrap().unwrap().balance, 4);
}
#[tokio::test]
async fn ordinary_signal_connection_and_streaming_subscription() {
    use operation_ledger::transport::{ConnectionServing, FrameCarrying, Input, Output};
    let dir = tempfile::tempdir().unwrap();
    let s = dir.open(HolderMode::Task, StubTranslator::default()).await;
    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let scope = s.clone();
    let serving = tokio::spawn(async move { server.serve(scope).await });
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&Input::Transfer(Transfer {
        from: "alice".into(),
        to: "bob".into(),
        amount: 5,
    }))
    .unwrap();
    client.write_frame(&bytes).await.unwrap();
    let reply = client.read_frame().await.unwrap();
    assert!(matches!(
        rkyv::from_bytes::<Output, rkyv::rancor::Error>(&reply).unwrap(),
        Output::Transferred
    ));
    serving.await.unwrap().unwrap();
    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let scope = s.clone();
    let serving = tokio::spawn(async move { server.serve(scope).await });
    client
        .write_frame(
            &rkyv::to_bytes::<rkyv::rancor::Error>(&Input::Subscribe("alice".into())).unwrap(),
        )
        .await
        .unwrap();
    let initial = client.read_frame().await.unwrap();
    assert!(matches!(
        rkyv::from_bytes::<Output, rkyv::rancor::Error>(&initial).unwrap(),
        Output::Account(Account { balance: 7, .. })
    ));
    s.timer(
        TimerFired {
            name: "alice".into(),
            amount: 3,
        },
        Duration::from_millis(1),
    )
    .await
    .unwrap()
    .unwrap();
    let updated = client.read_frame().await.unwrap();
    assert!(matches!(
        rkyv::from_bytes::<Output, rkyv::rancor::Error>(&updated).unwrap(),
        Output::Account(Account { balance: 10, .. })
    ));
    drop(client);
    serving.await.unwrap().unwrap();
    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let scope = s.clone();
    let serving = tokio::spawn(async move { server.serve(scope).await });
    client.write_frame(&[255]).await.unwrap();
    assert!(serving.await.unwrap().is_err());
    s.shutdown().await;
}
