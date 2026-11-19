use datom_codec::Datomizable;
use operation_ledger::{trace::Trace, transport::FrameCarrying};
use protos::{Protosizable, Textualizable};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("trace socket path required"))?;
    let mut stream = tokio::net::UnixStream::connect(path).await?;
    loop {
        let bytes = stream.read_frame().await?;
        if bytes.is_empty() {
            continue;
        }
        let trace = rkyv::from_bytes::<Trace, rkyv::rancor::Error>(&bytes)?;
        println!("{}", trace.datomize(Vec::new()).protosize().textualize());
    }
}
