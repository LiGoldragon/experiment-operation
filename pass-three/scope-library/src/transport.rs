use crate::*;
use std::future::Future;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Greeting {
    Greeted,
    Refused {
        received: [u8; 32],
        expected: [u8; 32],
    },
}
#[macro_export]
macro_rules! wire {
    ($t:ty) => {
        impl $crate::Wire for $t {
            fn encode(&self) -> Result<Vec<u8>, $crate::Failure> {
                rkyv::to_bytes::<rkyv::rancor::Error>(self)
                    .map(|x| x.to_vec())
                    .map_err(|_| $crate::Failure::Wire)
            }
            fn decode(bytes: &[u8]) -> Result<Self, $crate::Failure> {
                rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)
                    .map_err(|_| $crate::Failure::Wire)
            }
        }
    };
}
wire!(Greeting);
wire!(Failure);
wire!(Configuration);
pub trait FrameCarrying {
    fn write_frame(
        &mut self,
        bytes: &[u8],
        limit: u32,
    ) -> impl Future<Output = Result<(), Failure>> + Send;
    fn read_frame(&mut self, limit: u32) -> impl Future<Output = Result<Vec<u8>, Failure>> + Send;
}
impl FrameCarrying for UnixStream {
    async fn write_frame(&mut self, bytes: &[u8], limit: u32) -> Result<(), Failure> {
        let length = u32::try_from(bytes.len()).map_err(|_| Failure::Limit)?;
        if length > limit {
            return Err(Failure::Limit);
        }
        self.write_u32(length).await.map_err(|_| Failure::Wire)?;
        self.write_all(bytes).await.map_err(|_| Failure::Wire)
    }
    async fn read_frame(&mut self, limit: u32) -> Result<Vec<u8>, Failure> {
        let length = self.read_u32().await.map_err(|_| Failure::Wire)?;
        if length > limit {
            return Err(Failure::Limit);
        }
        let mut bytes = vec![0; length as usize];
        self.read_exact(&mut bytes)
            .await
            .map_err(|_| Failure::Wire)?;
        Ok(bytes)
    }
}
pub trait GreetingClient {
    fn greet(
        &mut self,
        fingerprint: [u8; 32],
        limit: u32,
    ) -> impl Future<Output = Result<Greeting, Failure>> + Send;
}
impl GreetingClient for UnixStream {
    async fn greet(&mut self, fingerprint: [u8; 32], limit: u32) -> Result<Greeting, Failure> {
        self.write_frame(&fingerprint, limit).await?;
        Greeting::decode(&self.read_frame(limit).await?)
    }
}
pub trait Questioning {
    fn ask<I: Wire, O: Wire>(
        &self,
        path: &str,
        fingerprint: [u8; 32],
        input: I,
    ) -> impl Future<Output = Result<O, Failure>> + Send;
}
impl<D: Domain> Questioning for Scope<D> {
    async fn ask<I: Wire, O: Wire>(
        &self,
        path: &str,
        fingerprint: [u8; 32],
        input: I,
    ) -> Result<O, Failure> {
        let limit = self.configuration.lock().unwrap().max_frame;
        let mut stream = UnixStream::connect(path)
            .await
            .map_err(|_| Failure::Stopped)?;
        match stream.greet(fingerprint, limit).await? {
            Greeting::Greeted => (),
            Greeting::Refused { received, expected } => {
                return Err(Failure::Schema {
                    stored: received,
                    expected,
                });
            }
        }
        stream.write_frame(&input.encode()?, limit).await?;
        O::decode(&stream.read_frame(limit).await?)
    }
}
pub trait Serving<D: Domain>: Clone + Send + Sync + 'static {
    type Input: Wire;
    type Output: Wire;
    fn dispatch(
        &self,
        input: Self::Input,
        scope: &Scope<D>,
    ) -> impl Future<Output = Result<Delivery<Self::Output>, Failure>> + Send;
}
pub trait SocketServing<D: Domain> {
    fn serve<S: Serving<D>>(
        &self,
        listener: UnixListener,
        service: S,
    ) -> impl Future<Output = Result<(), Failure>> + Send;
    fn serve_meta(
        &self,
        listener: UnixListener,
    ) -> impl Future<Output = Result<(), Failure>> + Send;
}
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum MetaInput {
    Configuration,
    MaxFrame(u32),
    CommandCapacity(u32),
    TraceCapacity(u32),
    MaxSubscriptions(u32),
    LockSocket(String),
    TranslatorSocket(String),
    FailCommit,
}
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum MetaOutput {
    Configured(Configuration),
    Refused(Failure),
}
wire!(MetaInput);
wire!(MetaOutput);
impl<D: Domain> SocketServing<D> for Scope<D> {
    async fn serve<S: Serving<D>>(
        &self,
        listener: UnixListener,
        service: S,
    ) -> Result<(), Failure> {
        let mut tasks = tokio::task::JoinSet::new();
        let mut dead = self.dead.clone();
        loop {
            tokio::select! {
                _=dead.changed()=>return Err(Failure::Stopped),
                connection=listener.accept()=>{let(mut stream,_)=connection.map_err(|_|Failure::Wire)?;let scope=self.clone();let service=service.clone();
                    tasks.spawn(async move {
                        let limit=scope.configuration.lock().unwrap().max_frame;
                        let received:[u8;32]=stream.read_frame(limit).await?.try_into().map_err(|_|Failure::Wire)?;
                        let greeting=if received==D::SIGNAL_FINGERPRINT {Greeting::Greeted}else{Greeting::Refused{received,expected:D::SIGNAL_FINGERPRINT}};
                        stream.write_frame(&greeting.encode()?,limit).await?;if !matches!(greeting,Greeting::Greeted){return Ok::<_,Failure>(())}
                        loop {let limit=scope.configuration.lock().unwrap().max_frame;let input=S::Input::decode(&stream.read_frame(limit).await?)?;
                            match service.dispatch(input,&scope).await? {
                                Delivery::Reply(output)=>stream.write_frame(&output.encode()?,limit).await?,
                                Delivery::Stream{mut receiver,map,lifecycle}=>{
                                    let _lifecycle=StreamLifecycle(lifecycle.clone());
                                    let initial=receiver.borrow_and_update().clone().ok_or(Failure::Wire)?;
                                    stream.write_frame(&map(&initial)?.encode()?,limit).await?;
                                    loop {
                                        let limit=scope.configuration.lock().unwrap().max_frame;
                                        SubscriptionDemand::decode(&stream.read_frame(limit).await?)?;
                                        receiver.changed().await.map_err(|_|Failure::Stopped)?;
                                        let bytes=receiver.borrow_and_update().clone().ok_or(Failure::Wire)?;
                                        if let Some(ref lifecycle)=lifecycle{lifecycle(SubscriptionEvent::Emitted(&bytes));}
                                        stream.write_frame(&map(&bytes)?.encode()?,limit).await?;
                                    }
                                }
                            }}
                    });},
                _=tasks.join_next(),if !tasks.is_empty()=>{}
            }
        }
    }
    async fn serve_meta(&self, listener: UnixListener) -> Result<(), Failure> {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|_| Failure::Wire)?;
            let limit = self.configuration.lock().unwrap().max_frame;
            let input = MetaInput::decode(&stream.read_frame(limit).await?)?;
            let output = {
                let mut configuration = self.configuration.lock().unwrap();
                let accepted = match input {
                    MetaInput::Configuration => true,
                    MetaInput::MaxFrame(x) if x > 0 => {
                        configuration.max_frame = x;
                        true
                    }
                    MetaInput::MaxSubscriptions(x) if x > 0 => {
                        configuration.max_subscriptions = x;
                        true
                    }
                    MetaInput::LockSocket(x) => {
                        configuration.lock_socket = x;
                        true
                    }
                    MetaInput::TranslatorSocket(x) => {
                        configuration.translator_socket = x;
                        true
                    }
                    MetaInput::FailCommit => {
                        self.fail_commit
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        true
                    }
                    // Channels are allocated at start; resizing requires a new place.
                    _ => false,
                };
                if accepted {
                    MetaOutput::Configured(configuration.clone())
                } else {
                    MetaOutput::Refused(Failure::Limit)
                }
            };
            stream.write_frame(&output.encode()?, limit).await?;
        }
    }
}

pub enum Delivery<O: Wire> {
    Reply(O),
    Stream {
        receiver: tokio::sync::watch::Receiver<Option<Vec<u8>>>,
        map: fn(&[u8]) -> Result<O, Failure>,
        lifecycle: Option<SubscriptionLifecycle>,
    },
}
struct StreamLifecycle(Option<SubscriptionLifecycle>);
impl Drop for StreamLifecycle {
    fn drop(&mut self) {
        if let Some(ref lifecycle) = self.0 {
            lifecycle(SubscriptionEvent::Ended);
        }
    }
}

#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SubscriptionDemand;
wire!(SubscriptionDemand);
