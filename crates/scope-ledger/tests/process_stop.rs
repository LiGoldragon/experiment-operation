use scope_ledger::{ledger::*, *};
use std::{marker::PhantomData, process::Command};

#[tokio::test]
async fn process_stop_keeps_committed_prefix_for_both_variants() {
    for variant in ["tasks", "actors"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.redb");
        let status = Command::new(env!("CARGO_BIN_EXE_scope-ledger"))
            .arg(variant)
            .arg(&path)
            .arg("crash")
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(77));
        let backend = if variant == "tasks" {
            Variant::Tasks
        } else {
            Variant::Actors
        };
        let scope = Scope::open(&path, backend, 16).unwrap();
        for (name, balance) in [("alice", 7), ("bob", 12)] {
            let account = scope
                .run(Recall::<Account> {
                    key: name.into(),
                    record: PhantomData,
                })
                .await
                .unwrap()
                .unwrap();
            assert_eq!(account.balance, balance);
        }
        scope.shutdown().await;
    }
}
