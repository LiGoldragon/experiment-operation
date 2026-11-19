use operation_scope::*;
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
};
use tokio::net::UnixStream;
use trial_lock::*;
const LIMIT: u32 = 1048576;
struct Machine {
    child: Child,
    trace: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for Machine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(task) = self.trace.take() {
            task.abort();
        }
    }
}
async fn receipt(child: &mut Child) -> u8 {
    let mut stdout = child.stdout.take().unwrap();
    let (stdout, byte) = tokio::task::spawn_blocking(move || {
        let mut byte = [0];
        stdout.read_exact(&mut byte).unwrap();
        (stdout, byte[0])
    })
    .await
    .unwrap();
    child.stdout = Some(stdout);
    byte
}
async fn machine(root: &Path) -> Machine {
    for name in ["lock.sock", "lock.meta", "lock.trace"] {
        let _ = std::fs::remove_file(root.join(name));
    }
    let configuration = Configuration {
        max_frame: LIMIT,
        command_capacity: 64,
        trace_capacity: 8192,
        max_subscriptions: 16,
        lock_socket: String::new(),
        translator_socket: String::new(),
        observer_socket: String::new(),
        pause_at: None,
    };
    let launch = LaunchConfiguration {
        store: root.join("memory").display().to_string(),
        ordinary_socket: root.join("lock.sock").display().to_string(),
        meta_socket: root.join("lock.meta").display().to_string(),
        trace_socket: root.join("lock.trace").display().to_string(),
        configuration,
        worker_threads: 2,
        startup_receipt: true,
    };
    let path = root.join("launch");
    std::fs::write(&path, launch.encode().unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_trial-lock-nexus"))
        .arg(path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut byte = receipt(&mut child).await;
    let trace = if byte == 0 {
        let mut stream = UnixStream::connect(root.join("lock.trace")).await.unwrap();
        assert!(stream.read_frame(LIMIT).await.unwrap().is_empty());
        let task = tokio::spawn(async move {
            while let Ok(bytes) = stream.read_frame(LIMIT).await {
                assert!(TraceRecord::decode(&bytes).is_ok());
            }
        });
        byte = receipt(&mut child).await;
        Some(task)
    } else {
        None
    };
    assert_eq!(byte, 1);
    Machine { child, trace }
}
async fn acquire(root: &Path) -> UnixStream {
    let mut stream = UnixStream::connect(root.join("lock.sock")).await.unwrap();
    assert_eq!(
        stream.greet(SIGNAL_FINGERPRINT, LIMIT).await.unwrap(),
        Greeting::Greeted
    );
    stream
        .write_frame(
            &Input::Lock(Lock {
                lock_name: LockName("owned".into()),
            })
            .encode()
            .unwrap(),
            LIMIT,
        )
        .await
        .unwrap();
    assert!(matches!(
        Output::decode(&stream.read_frame(LIMIT).await.unwrap()).unwrap(),
        Output::Locked(_)
    ));
    stream
}
#[tokio::test]
async fn killed_lock_owner_purges_persisted_holding_on_start() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let owner = machine(root).await;
    let old_exchange = acquire(root).await;
    drop(owner); // SIGKILL: no guard destructor runs in the Nexus.
    let restarted = machine(root).await;
    let new_exchange = acquire(root).await;
    drop(old_exchange);
    drop(new_exchange);
    drop(restarted);
}
