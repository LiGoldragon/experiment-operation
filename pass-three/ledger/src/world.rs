//! Bodies for the durable external-world stub. Its journal is not ledger Memory.
use crate::*;
use operation_scope::*;
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct WorldLaunch {
    pub journal: String,
    pub socket: String,
    pub configuration: Configuration,
    pub succeeds: bool,
}
operation_scope::wire!(WorldLaunch);
#[derive(Clone)]
pub struct World {
    pub journal: Journal,
    pub succeeds: bool,
}
pub trait WorldRunning {
    fn run();
}
impl WorldRunning for World {
    fn run() {
        let result = (|| {
            let launch = WorldLaunch::decode(
                &std::fs::read(std::env::args_os().nth(1).ok_or(Failure::Wire)?)
                    .map_err(|_| Failure::Wire)?,
            )?;
            let journal = Journal::open(&launch.journal)?;
            let world = Self {
                journal,
                succeeds: launch.succeeds,
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| Failure::Stopped)?;
            runtime.block_on(async move {
                let listener =
                    tokio::net::UnixListener::bind(&launch.socket).map_err(|_| Failure::Wire)?;
                // Separate owner solely for shared serving and configured boundary events.
                let scope = Scope::<NexusDomain>::open(
                    format!("{}.scope", launch.journal).into(),
                    launch.configuration.clone(),
                    TraceHub::open(launch.configuration.trace_capacity as usize),
                )
                .await?;
                {
                    use std::io::Write;
                    std::io::stdout()
                        .write_all(&[1])
                        .map_err(|_| Failure::Wire)?;
                    std::io::stdout().flush().map_err(|_| Failure::Wire)?;
                }
                scope.serve(listener, world).await
            })
        })();
        if result.is_err() {
            std::process::exit(1)
        }
    }
}
impl Serving<NexusDomain> for World {
    type Input = WorldInput;
    type Output = RemitOutcome;
    async fn dispatch(
        &self,
        input: WorldInput,
        s: &Scope<NexusDomain>,
    ) -> Result<Delivery<RemitOutcome>, Failure> {
        let outcome = match input {
            WorldInput::Status(status) => {
                let journal = self.journal.clone();
                let key = status.reservation_id.encode()?;
                tokio::task::spawn_blocking(move || journal.outcome(&key))
                    .await
                    .map_err(|_| Failure::Stopped)??
                    .unwrap_or(RemitOutcome::Unknown)
            }
            WorldInput::Remit(remit) => {
                s.boundary(Boundary::BeforeJournal).await?;
                let journal = self.journal.clone();
                let key = remit.reservation.reservation_id.encode()?;
                let outcome = if self.succeeds {
                    RemitOutcome::Remitted
                } else {
                    RemitOutcome::Unremitted(1)
                };
                let outcome = tokio::task::spawn_blocking(move || journal.record(&key, outcome))
                    .await
                    .map_err(|_| Failure::Stopped)??;
                s.boundary(Boundary::AfterJournal).await?;
                outcome
            }
        };
        Ok(Delivery::Reply(outcome))
    }
}
