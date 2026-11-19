//! Generator template: optional text executables, absent from ordinary builds.
pub(crate) const TEXT_CLIENTS: &str = r#"
#[cfg(feature="datom")]
pub use text_clients::{Client,MetaClient,Debugger,TextRunning};
#[cfg(feature="datom")]
mod text_clients {
    use super::*;
    use operation_scope::{Wire,FrameCarrying,GreetingClient};
    use datom_codec::{Actualizing,Budget,Composing,Datomizable,Potential};
    use protos::{ReaderBudget,Protosizable,Textualizable};
    pub struct Client;
    pub struct MetaClient;
    pub struct Debugger;
    pub trait TextRunning {fn run();}
    enum Surface { Ordinary,Meta,Trace }
    struct Invocation {launch:operation_scope::LaunchConfiguration,request:Request}
    enum Request {Ordinary(Input),Meta(operation_scope::MetaInput),Trace}
    trait DatomReading {fn read<T:Composing>(&self,limit:u32)->Result<T,operation_scope::Failure>;}
    impl DatomReading for str {
        fn read<T:Composing>(&self,limit:u32)->Result<T,operation_scope::Failure> {
            if self.len()>limit as usize{return Err(operation_scope::Failure::Limit)}
            let size=self.len().saturating_add(1);
            let bound=i64::try_from(size).map_err(|_|operation_scope::Failure::Limit)?;
            let mut budget=Budget {remaining:bound,reader:ReaderBudget::try_from(size).map_err(|_|operation_scope::Failure::Limit)?,depth:0,maximum_depth:bound};
            Potential::<T>::from(self).actualize(&mut budget).map_err(|_|operation_scope::Failure::Wire)
        }
    }
    trait Launching {fn launch(self);}
    impl Launching for Surface {
        fn launch(self) {
            let result=(|| {
                let mut args=std::env::args().skip(1);
                let path=args.next().ok_or(operation_scope::Failure::Wire)?;
                let launch=operation_scope::LaunchConfiguration::decode(&std::fs::read(path).map_err(|_|operation_scope::Failure::Wire)?)?;
                let limit=launch.configuration.max_frame;
                let request=match self {
                    Surface::Ordinary=>Request::Ordinary(args.next().ok_or(operation_scope::Failure::Wire)?.read(limit)?),
                    Surface::Meta=>Request::Meta(args.next().ok_or(operation_scope::Failure::Wire)?.read(limit)?),
                    Surface::Trace=>Request::Trace,
                };
                if args.next().is_some()||launch.worker_threads==0{return Err(operation_scope::Failure::Wire)}
                let runtime=tokio::runtime::Builder::new_multi_thread().worker_threads(launch.worker_threads as usize).enable_all().build().map_err(|_|operation_scope::Failure::Stopped)?;
                runtime.block_on(Invocation{launch,request}.process())
            })();
            if let Err(error)=result {eprintln!("{error:?}");std::process::exit(1)}
        }
    }
    impl TextRunning for Client {fn run(){Surface::Ordinary.launch()}}
    impl TextRunning for MetaClient {fn run(){Surface::Meta.launch()}}
    impl TextRunning for Debugger {fn run(){Surface::Trace.launch()}}
    trait Processing {fn process(self)->impl std::future::Future<Output=Result<(),operation_scope::Failure>>;}
    impl Processing for Invocation {
        async fn process(self)->Result<(),operation_scope::Failure> {
            let limit=self.launch.configuration.max_frame;
            match self.request {
                Request::Ordinary(input)=>{
                    let streaming=IS_SUBSCRIPTION;
                    let mut stream=tokio::net::UnixStream::connect(&self.launch.ordinary_socket).await.map_err(|_|operation_scope::Failure::Stopped)?;
                    if !matches!(stream.greet(SIGNAL_FINGERPRINT,limit).await?,operation_scope::Greeting::Greeted){return Err(operation_scope::Failure::Wire)}
                    stream.write_frame(&input.encode()?,limit).await?;
                    let mut demand:Option<operation_scope::SubscriptionDemand>=None;
                    loop {if let Some(demand)=demand {stream.write_frame(&demand.encode()?,limit).await?;}
                        let output=Output::decode(&stream.read_frame(limit).await?)?;
                        println!("{}",output.datomize(vec![]).protosize().textualize());
                        if !streaming {break;}
                        demand=Some(operation_scope::SubscriptionDemand);
                    }
                },
                Request::Meta(input)=>{
                    let mut stream=tokio::net::UnixStream::connect(&self.launch.meta_socket).await.map_err(|_|operation_scope::Failure::Stopped)?;
                    stream.write_frame(&input.encode()?,limit).await?;
                    let output=operation_scope::MetaOutput::decode(&stream.read_frame(limit).await?)?;
                    println!("{}",output.datomize(vec![]).protosize().textualize());
                },
                Request::Trace=>{
                    let mut stream=tokio::net::UnixStream::connect(&self.launch.trace_socket).await.map_err(|_|operation_scope::Failure::Stopped)?;
                    loop {let bytes=stream.read_frame(limit).await?;if bytes.is_empty(){continue;}
                        let trace=TraceRecord::decode(&bytes)?;
                        println!("{}",trace.datomize(vec![]).protosize().textualize());
                    }
                },
            }
            Ok(())
        }
    }
}
"#;
