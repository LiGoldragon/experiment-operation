use crate::*;
use std::future::Future;
use tokio::net::UnixListener;
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LaunchConfiguration {
    pub store: String,
    pub ordinary_socket: String,
    pub meta_socket: String,
    pub trace_socket: String,
    pub configuration: Configuration,
    pub worker_threads: u32,
    pub startup_receipt: bool,
}
wire!(LaunchConfiguration);
pub trait Starting<D: Domain> {
    fn start(&self, scope: &Scope<D>) -> impl Future<Output = Result<(), Failure>> + Send;
}
pub trait TraceEncoding<D: Domain> {
    fn encode_trace(trace: Trace<D>) -> Result<Vec<u8>, Failure>;
}
pub trait ProcessRunning<D: Domain> {
    fn run(self);
}
pub trait ProcessServing<D: Domain> {
    fn process(
        self,
        launch: LaunchConfiguration,
    ) -> impl Future<Output = Result<(), Failure>> + Send;
}
impl<D: Domain, S> ProcessRunning<D> for S
where
    S: Serving<D> + Starting<D> + TraceEncoding<D>,
{
    fn run(self) {
        let result = (|| {
            let path = std::env::args_os().nth(1).ok_or(Failure::Wire)?;
            let launch =
                LaunchConfiguration::decode(&std::fs::read(path).map_err(|_| Failure::Wire)?)?;
            if launch.worker_threads == 0 {
                return Err(Failure::Limit);
            }
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(launch.worker_threads as usize)
                .enable_all()
                .build()
                .map_err(|_| Failure::Stopped)?;
            runtime.block_on(self.process(launch))
        })();
        if result.is_err() {
            std::process::exit(1)
        }
    }
}
impl<D: Domain, S> ProcessServing<D> for S
where
    S: Serving<D> + Starting<D> + TraceEncoding<D>,
{
    async fn process(self, launch: LaunchConfiguration) -> Result<(), Failure> {
        let traces = TraceHub::<D>::open(launch.configuration.trace_capacity as usize);
        #[cfg(feature = "trace")]
        let trace_task = {
            // Bind and admit debugger before opening memory or resuming anything.
            let listener = UnixListener::bind(&launch.trace_socket).map_err(|_| Failure::Wire)?;
            if launch.startup_receipt {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&[0])
                    .map_err(|_| Failure::Wire)?;
                std::io::stdout().flush().map_err(|_| Failure::Wire)?;
            }
            let (mut stream, _) = listener.accept().await.map_err(|_| Failure::Wire)?;
            let mut receiver = traces.sender.subscribe();
            let limit = launch.configuration.max_frame;
            stream.write_frame(&[], limit).await?;
            tokio::spawn(async move {
                loop {
                    let trace = receiver.recv().await.map_err(|_| Failure::Limit)?;
                    stream.write_frame(&S::encode_trace(trace)?, limit).await?;
                }
                #[allow(unreachable_code)]
                Ok::<_, Failure>(())
            })
        };
        let scope = Scope::<D>::open(launch.store.into(), launch.configuration, traces).await?;
        self.start(&scope).await?;
        let ordinary = UnixListener::bind(launch.ordinary_socket).map_err(|_| Failure::Wire)?;
        let meta = UnixListener::bind(launch.meta_socket).map_err(|_| Failure::Wire)?;
        if launch.startup_receipt {
            use std::io::Write;
            std::io::stdout()
                .write_all(&[1])
                .map_err(|_| Failure::Wire)?;
            std::io::stdout().flush().map_err(|_| Failure::Wire)?;
        }
        #[cfg(feature = "trace")]
        {
            tokio::select! {result=scope.serve(ordinary,self)=>result,result=scope.serve_meta(meta)=>result,_=trace_task=>Err(Failure::Stopped)}
        }
        #[cfg(not(feature = "trace"))]
        {
            tokio::select! {result=scope.serve(ordinary,self)=>result,result=scope.serve_meta(meta)=>result}
        }
    }
}
