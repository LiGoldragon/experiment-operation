use datom_codec::{Composing, Datomizable};
use experiment_upgrade::DatomReading;
use operation_ledger::{Runnable, ledger::*, scope::*, transport::HolderMode};
#[derive(Composing, Datomizable)]
enum Command {
    Seed {
        store: String,
    },
    CrashPay {
        store: String,
    },
    Resume {
        store: String,
        remitted: bool,
    },
    Benchmark {
        store: String,
        actor: bool,
        count: i64,
    },
    HolderBenchmark {
        actor: bool,
        count: i64,
    },
    Serve {
        store: String,
        socket: String,
        meta: String,
        trace: String,
        actor: bool,
    },
}
trait Executing {
    fn execute(self) -> impl std::future::Future<Output = anyhow::Result<()>>;
}
impl Executing for Command {
    async fn execute(self) -> anyhow::Result<()> {
        match self {
            Command::Seed { store } => {
                let s = Scope::open(store.into(), HolderMode::Task, StubTranslator::default())
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                s.run(Create {
                    account: Account {
                        name: "alice".into(),
                        balance: 12,
                    },
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                s.shutdown().await;
            }
            Command::CrashPay { store } => {
                let t = StubTranslator {
                    behavior: RemitBehavior::Never,
                    ..Default::default()
                };
                let reached = t.reached.clone();
                let s = Scope::open(store.into(), HolderMode::Task, t)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                tokio::spawn(async move {
                    s.run(Pay {
                        reservation_id: "payment".into(),
                        name: "alice".into(),
                        amount: 5,
                    })
                    .await
                });
                reached.notified().await;
                // Abrupt termination deliberately bypasses shutdown and destructors.
                std::process::exit(77);
            }
            Command::Resume { store, remitted } => {
                let t = StubTranslator::default();
                t.journal.lock().unwrap().insert(
                    "payment".into(),
                    if remitted {
                        RemitOutcome::Remitted
                    } else {
                        RemitOutcome::Unremitted(9)
                    },
                );
                let s = Scope::open(store.into(), HolderMode::Task, t)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                let a = s
                    .run(Inspect {
                        name: "alice".into(),
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                println!("{}", a.balance);
                s.shutdown().await;
            }
            Command::Benchmark {
                store,
                actor,
                count,
            } => {
                use operation_ledger::{scope::Subscribing, transport::SubscriptionReceiving};
                let s = Scope::open(
                    store.into(),
                    if actor {
                        HolderMode::Actor
                    } else {
                        HolderMode::Task
                    },
                    StubTranslator::default(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                s.run(Create {
                    account: Account {
                        name: "alice".into(),
                        balance: 0,
                    },
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                let mut subscription = s
                    .subscribe("alice".into())
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                subscription.next().await;
                let start = std::time::Instant::now();
                for _ in 0..count {
                    s.run(Deposit {
                        name: "alice".into(),
                        amount: 1,
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                    subscription.next().await;
                }
                println!("{} {}", count, start.elapsed().as_nanos());
                drop(subscription);
                s.shutdown().await;
            }
            Command::HolderBenchmark { actor, count } => {
                use operation_ledger::transport::{
                    Holder, HolderDispatch, HolderOpening, SubscriberCommand, SubscriptionEntry,
                };
                anyhow::ensure!(count > 0, "count must be positive");
                let holder = Holder::open(if actor {
                    HolderMode::Actor
                } else {
                    HolderMode::Task
                });
                let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
                holder
                    .dispatch(SubscriberCommand::Open(SubscriptionEntry {
                        id: 0,
                        account: Account {
                            name: "alice".into(),
                            balance: 0,
                        },
                        sender,
                        #[cfg(feature = "trace")]
                        context: Default::default(),
                    }))
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                receiver.recv().await;
                let start = std::time::Instant::now();
                for balance in 0..count {
                    holder
                        .dispatch(SubscriberCommand::Changed(Account {
                            name: "alice".into(),
                            balance,
                        }))
                        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                    receiver.recv().await;
                }
                println!("{} {}", count, start.elapsed().as_nanos());
                holder.close().await;
            }
            Command::Serve {
                store,
                socket,
                meta,
                trace,
                actor,
            } => {
                use operation_ledger::transport::{LedgerServing, Server};
                let scope = Scope::open(
                    store.into(),
                    if actor {
                        HolderMode::Actor
                    } else {
                        HolderMode::Task
                    },
                    StubTranslator::default(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                Server {
                    scope,
                    socket: socket.into(),
                    meta: meta.into(),
                    trace: trace.into(),
                }
                .serve()
                .await?;
            }
        }
        Ok(())
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("one datom command is required"))?;
    anyhow::ensure!(args.next().is_none(), "takes one datom");
    command.as_str().read_datom::<Command>()?.execute().await
}
