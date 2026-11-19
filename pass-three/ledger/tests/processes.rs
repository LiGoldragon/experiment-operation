use operation_scope::*;
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
};
use tokio::net::{UnixListener, UnixStream};
use trial_ledger::*;
const LIMIT: u32 = 1048576;
fn config(root: &Path, pause: Option<Boundary>) -> Configuration {
    Configuration {
        max_frame: LIMIT,
        command_capacity: 64,
        trace_capacity: 8192,
        max_subscriptions: 16,
        lock_socket: root.join("lock.sock").display().to_string(),
        translator_socket: root.join("world.sock").display().to_string(),
        observer_socket: root.join("observer.sock").display().to_string(),
        pause_at: pause,
    }
}
struct Machine {
    child: Child,
    traces: Option<tokio::task::JoinHandle<()>>,
}
impl Machine {
    async fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(task) = self.traces.take() {
            task.abort();
        }
    }
}
impl Drop for Machine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(task) = self.traces.take() {
            task.abort();
        }
    }
}
async fn read_receipt(child: &mut Child) -> [u8; 1] {
    let mut stdout = child.stdout.take().unwrap();
    let (stdout, receipt) = tokio::task::spawn_blocking(move || {
        let mut receipt = [0];
        stdout.read_exact(&mut receipt).unwrap();
        (stdout, receipt)
    })
    .await
    .unwrap();
    child.stdout = Some(stdout);
    receipt
}
async fn ledger(root: &Path, pause: Option<Boundary>) -> Machine {
    for name in ["ledger.sock", "ledger.meta", "ledger.trace"] {
        let _ = std::fs::remove_file(root.join(name));
    }
    let launch = LaunchConfiguration {
        store: root.join("ledger.memory").display().to_string(),
        ordinary_socket: root.join("ledger.sock").display().to_string(),
        meta_socket: root.join("ledger.meta").display().to_string(),
        trace_socket: root.join("ledger.trace").display().to_string(),
        configuration: config(root, pause),
        worker_threads: 2,
        startup_receipt: true,
    };
    let path = root.join("ledger.launch");
    std::fs::write(&path, launch.encode().unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_trial-ledger-nexus"))
        .arg(path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut receipt = read_receipt(&mut child).await;
    let traces = if receipt[0] == 0 {
        let mut stream = UnixStream::connect(root.join("ledger.trace"))
            .await
            .unwrap();
        assert!(stream.read_frame(LIMIT).await.unwrap().is_empty());
        let task = tokio::spawn(async move {
            while let Ok(bytes) = stream.read_frame(LIMIT).await {
                assert!(TraceRecord::decode(&bytes).is_ok());
            }
        });
        receipt = read_receipt(&mut child).await;
        Some(task)
    } else {
        None
    };
    assert_eq!(receipt, [1]);
    Machine { child, traces }
}
async fn world(root: &Path, pause: Option<Boundary>, succeeds: bool) -> Machine {
    let _ = std::fs::remove_file(root.join("world.sock"));
    let launch = trial_ledger::world::WorldLaunch {
        journal: root.join("world.memory").display().to_string(),
        socket: root.join("world.sock").display().to_string(),
        configuration: config(root, pause),
        succeeds,
    };
    let path = root.join("world.launch");
    std::fs::write(&path, launch.encode().unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_trial-world"))
        .arg(path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let receipt = read_receipt(&mut child).await;
    assert_eq!(receipt, [1]);
    Machine {
        child,
        traces: None,
    }
}
async fn try_call(root: &Path, input: Input) -> Result<Output, Failure> {
    let mut stream = UnixStream::connect(root.join("ledger.sock"))
        .await
        .map_err(|_| Failure::Wire)?;
    assert_eq!(
        stream.greet(SIGNAL_FINGERPRINT, LIMIT).await?,
        Greeting::Greeted
    );
    stream.write_frame(&input.encode()?, LIMIT).await?;
    Output::decode(&stream.read_frame(LIMIT).await?)
}
async fn call(root: &Path, input: Input) -> Output {
    try_call(root, input).await.unwrap()
}
async fn locks(root: &Path) -> tokio::task::JoinHandle<()> {
    let scope = Scope::<trial_lock::NexusDomain>::open(
        root.join("lock.memory"),
        config(root, None),
        TraceHub::open(8192),
    )
    .await
    .unwrap();
    let listener = UnixListener::bind(root.join("lock.sock")).unwrap();
    tokio::spawn(async move {
        let _ = scope.serve(listener, trial_lock::Nexus).await;
    })
}
async fn seed(root: &Path) {
    assert!(matches!(
        call(
            root,
            Input::Create(Create {
                account: Account {
                    name: Name("alice".into()),
                    balance: Balance(12)
                }
            })
        )
        .await,
        Output::Created(_)
    ));
}
async fn balance(root: &Path) -> i64 {
    match call(
        root,
        Input::Inspect(Inspect {
            name: Name("alice".into()),
        }),
    )
    .await
    {
        Output::Account(account) => account.balance.0,
        other => panic!("{other:?}"),
    }
}
async fn boundary(listener: &UnixListener, expected: Boundary) -> UnixStream {
    let (mut stream, _) = listener.accept().await.unwrap();
    assert_eq!(
        Boundary::decode(&stream.read_frame(LIMIT).await.unwrap()).unwrap(),
        expected
    );
    stream
}

#[tokio::test]
async fn mismatched_greeting_refuses_before_reading_input() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let locks = locks(root).await;
    let mut stream = UnixStream::connect(root.join("lock.sock")).await.unwrap();
    let received = [9; 32];
    assert_eq!(
        stream.greet(received, LIMIT).await.unwrap(),
        Greeting::Refused {
            received,
            expected: trial_lock::SIGNAL_FINGERPRINT
        }
    );
    assert!(stream.read_frame(LIMIT).await.is_err());
    locks.abort();
}
#[tokio::test]
async fn kill_ledger_at_each_boundary_and_resume_before_inputs() {
    for pause in [
        Boundary::Reserved,
        Boundary::BeforeCall,
        Boundary::AfterReply,
        Boundary::BeforeSettle,
        Boundary::AfterSettle,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let locks = locks(&root).await;
        let world = world(&root, None, true).await;
        let observer = UnixListener::bind(root.join("observer.sock")).unwrap();
        let machine = ledger(&root, Some(pause.clone())).await;
        seed(&root).await;
        let calling = root.clone();
        let pay = tokio::spawn(async move {
            try_call(
                &calling,
                Input::Pay(Pay {
                    reservation: Reservation {
                        reservation_id: ReservationId("once".into()),
                        name: Name("alice".into()),
                        amount: Amount(5),
                    },
                }),
            )
            .await
        });
        let _barrier = boundary(&observer, pause).await;
        machine.stop().await;
        pay.abort();
        let machine = ledger(&root, None).await;
        assert_eq!(balance(&root).await, 7);
        // Settled identifiers are retained, so a retry cannot debit or remit again.
        assert!(matches!(
            call(
                &root,
                Input::Pay(Pay {
                    reservation: Reservation {
                        reservation_id: ReservationId("once".into()),
                        name: Name("alice".into()),
                        amount: Amount(5)
                    }
                })
            )
            .await,
            Output::Refused(Refused::Duplicate(_))
        ));
        machine.stop().await;
        world.stop().await;
        let journal = Journal::open(&root.join("world.memory").display().to_string()).unwrap();
        assert_eq!(journal.entries().unwrap(), 1);
        locks.abort();
    }
}
#[tokio::test]
async fn kill_world_before_and_after_journal_then_restart_both() {
    for pause in [Boundary::BeforeJournal, Boundary::AfterJournal] {
        for succeeds in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let locks = locks(&root).await;
            let observer = UnixListener::bind(root.join("observer.sock")).unwrap();
            let translator = world(&root, Some(pause.clone()), succeeds).await;
            let machine = ledger(&root, None).await;
            seed(&root).await;
            let calling = root.clone();
            let pay = tokio::spawn(async move {
                try_call(
                    &calling,
                    Input::Pay(Pay {
                        reservation: Reservation {
                            reservation_id: ReservationId("once".into()),
                            name: Name("alice".into()),
                            amount: Amount(5),
                        },
                    }),
                )
                .await
            });
            let _barrier = boundary(&observer, pause.clone()).await;
            translator.stop().await;
            machine.stop().await;
            pay.abort();
            let translator = world(&root, None, succeeds).await;
            let machine = ledger(&root, None).await;
            assert_eq!(balance(&root).await, if succeeds { 7 } else { 12 });
            machine.stop().await;
            translator.stop().await;
            let journal = Journal::open(&root.join("world.memory").display().to_string()).unwrap();
            assert_eq!(journal.entries().unwrap(), 1);
            locks.abort();
        }
    }
}

#[tokio::test]
async fn socket_subscription_reads_latest_once_after_thousand_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let machine = ledger(root, None).await;
    seed(root).await;
    let mut stream = UnixStream::connect(root.join("ledger.sock")).await.unwrap();
    assert_eq!(
        stream.greet(SIGNAL_FINGERPRINT, LIMIT).await.unwrap(),
        Greeting::Greeted
    );
    stream
        .write_frame(
            &Input::Subscribe(Subscribe {
                name: Name("alice".into()),
            })
            .encode()
            .unwrap(),
            LIMIT,
        )
        .await
        .unwrap();
    assert!(matches!(
        Output::decode(&stream.read_frame(LIMIT).await.unwrap()).unwrap(),
        Output::Subscribed(Subscribed {
            account: Account {
                balance: Balance(12),
                ..
            }
        })
    ));
    for _ in 0..1000 {
        assert!(matches!(
            call(
                root,
                Input::Deposit(Deposit {
                    name: Name("alice".into()),
                    amount: Amount(1)
                })
            )
            .await,
            Output::Deposited(_)
        ));
    }
    stream
        .write_frame(&SubscriptionDemand.encode().unwrap(), LIMIT)
        .await
        .unwrap();
    assert!(matches!(
        Output::decode(&stream.read_frame(LIMIT).await.unwrap()).unwrap(),
        Output::Subscribed(Subscribed {
            account: Account {
                balance: Balance(1012),
                ..
            }
        })
    ));
    machine.stop().await;
}

#[cfg(feature = "datom")]
#[tokio::test]
async fn generated_text_clients_reach_ordinary_and_meta_sockets() {
    use datom_codec::Datomizable;
    use protos::{Protosizable, Textualizable};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let machine = ledger(root, None).await;
    seed(root).await;
    let request = Input::Inspect(Inspect {
        name: Name("alice".into()),
    })
    .datomize(vec![])
    .protosize()
    .textualize();
    let result = Command::new(env!("CARGO_BIN_EXE_trial-ledger-client"))
        .arg(root.join("ledger.launch"))
        .arg(request)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("Account"));
    let result = Command::new(env!("CARGO_BIN_EXE_trial-ledger-meta"))
        .arg(root.join("ledger.launch"))
        .arg("MaxFrame.2048")
        .output()
        .unwrap();
    assert!(result.status.success());
    let mut stream = UnixStream::connect(root.join("ledger.meta")).await.unwrap();
    stream
        .write_frame(&MetaInput::Configuration.encode().unwrap(), LIMIT)
        .await
        .unwrap();
    assert!(matches!(
        MetaOutput::decode(&stream.read_frame(LIMIT).await.unwrap()).unwrap(),
        MetaOutput::Configured(Configuration {
            max_frame: 2048,
            ..
        })
    ));
    machine.stop().await;
}
