#![cfg(feature = "datom")]
#[test]
fn abrupt_process_stop_preserves_reservation_and_resume_asks_world() {
    for remitted in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        let path = store.display();
        let binary = env!("CARGO_BIN_EXE_operation-ledger");
        assert!(
            std::process::Command::new(binary)
                .arg(format!("Seed.{path}"))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            std::process::Command::new(binary)
                .arg(format!("CrashPay.{path}"))
                .status()
                .unwrap()
                .code(),
            Some(77)
        );
        let out = std::process::Command::new(binary)
            .arg(format!(
                "Resume.{{ {path} {} }}",
                if remitted { "True" } else { "False" }
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8(out.stdout).unwrap().trim(),
            if remitted { "7" } else { "12" }
        );
    }
}
