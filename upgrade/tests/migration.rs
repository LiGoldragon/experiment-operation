use experiment_upgrade::*;
use std::{fs, process::Command};

#[allow(dead_code)]
mod generated {
    include!("fixtures/generated.rs");
}

trait FixtureReading {
    fn manifest(&self) -> Manifest;
}
impl FixtureReading for str {
    fn manifest(&self) -> Manifest {
        self.read_datom().unwrap()
    }
}

#[test]
fn generated_fixture_is_exactly_the_manifest_product() {
    let manifest = include_str!("../examples/manifest.datom").manifest();
    assert_eq!(
        manifest.generate_rust().unwrap(),
        include_str!("fixtures/generated.rs")
    );
    assert_eq!(manifest.schemas().unwrap().len(), 8);
}

#[test]
fn version_zero_to_two_commits_and_reopens() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("memory.redb");
    let store = Store::open(path.clone()).unwrap();
    store.initialize().unwrap();
    store
        .insert(&generated::v0::Account {
            r#name: "alice".into(),
            r#balance: 12,
            r#status: generated::v0::Status::Open,
        })
        .unwrap();
    store
        .insert(&generated::v0::Account {
            r#name: "bob".into(),
            r#balance: 3,
            r#status: generated::v0::Status::Frozen,
        })
        .unwrap();
    let mut observer = FaultInjection {
        fail_after: None,
        writes: 0,
    };
    store
        .upgrade(&generated::UpgradePlan { target: 2 }, &mut observer)
        .unwrap();
    assert_eq!(observer.writes, 4);
    drop(store);
    let store = Store::open(path).unwrap();
    assert_eq!(store.version().unwrap(), 2);
    let records = store.records::<generated::v2::LedgerAccount>().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].r#name, "alice");
    assert_eq!(records[0].r#balance, 12);
    assert_eq!(records[0].r#limit, 100);
    assert_eq!(records[1].r#balance, 3);
    assert_eq!(records[1].r#status, generated::v2::Status::Frozen);
    // Historical record types cannot read a store stamped at a newer version.
    assert!(store.records::<generated::v0::Account>().is_err());
}

#[test]
fn failure_at_every_write_leaves_original_archives_and_stamp() {
    for fail_after in 1..=4 {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.redb");
        let store = Store::open(path.clone()).unwrap();
        store.initialize().unwrap();
        let originals = vec![
            generated::v0::Account {
                r#name: "alice".into(),
                r#balance: 12,
                r#status: generated::v0::Status::Open,
            },
            generated::v0::Account {
                r#name: "bob".into(),
                r#balance: 3,
                r#status: generated::v0::Status::Frozen,
            },
        ];
        for record in &originals {
            store.insert(record).unwrap();
        }
        let mut observer = FaultInjection {
            fail_after: Some(fail_after),
            writes: 0,
        };
        assert!(
            store
                .upgrade(&generated::UpgradePlan { target: 2 }, &mut observer)
                .unwrap_err()
                .to_string()
                .contains("injected failure")
        );
        assert_eq!(observer.writes, fail_after);
        drop(store);
        let store = Store::open(path).unwrap();
        assert_eq!(store.version().unwrap(), 0);
        let records = store.records::<generated::v0::Account>().unwrap();
        assert_eq!(records, originals);
        for (actual, original) in records.iter().zip(&originals) {
            assert_eq!(actual.encode().unwrap(), original.encode().unwrap());
        }
        // The same failed store remains upgradeable without any repair.
        store
            .upgrade(
                &generated::UpgradePlan { target: 2 },
                &mut FaultInjection {
                    fail_after: None,
                    writes: 0,
                },
            )
            .unwrap();
        assert_eq!(store.version().unwrap(), 2);
    }
}

#[test]
fn all_seven_edits_compile_and_upgrade_from_an_intermediate_version() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("memory.redb")).unwrap();
    store.initialize().unwrap();
    store
        .insert(&generated::v0::Account {
            r#name: "alice".into(),
            r#balance: 12,
            r#status: generated::v0::Status::Frozen,
        })
        .unwrap();
    store
        .upgrade(
            &generated::UpgradePlan { target: 2 },
            &mut FaultInjection {
                fail_after: None,
                writes: 0,
            },
        )
        .unwrap();
    store
        .upgrade(
            &generated::UpgradePlan { target: 7 },
            &mut FaultInjection {
                fail_after: None,
                writes: 0,
            },
        )
        .unwrap();
    let records = store.records::<generated::v7::LedgerAccount>().unwrap();
    assert_eq!(records[0].r#amount, 13);
    assert_eq!(records[0].r#note, "reshaped");
    assert_eq!(records[0].r#status, generated::v7::Status::Closed);
    assert!(
        store
            .upgrade(
                &generated::UpgradePlan { target: 2 },
                &mut FaultInjection {
                    fail_after: None,
                    writes: 0
                }
            )
            .is_err()
    );
    assert!(
        store
            .upgrade(
                &generated::UpgradePlan { target: 8 },
                &mut FaultInjection {
                    fail_after: None,
                    writes: 0
                }
            )
            .is_err()
    );
    assert_eq!(store.version().unwrap(), 7);
    assert!(store.initialize().is_err());
}

#[test]
fn bad_archive_refuses_before_any_stamp_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("memory.redb");
    {
        let store = Store::open(path.clone()).unwrap();
        store.initialize().unwrap();
    }
    {
        let database = redb::Database::create(&path).unwrap();
        let tx = database.begin_write().unwrap();
        {
            let mut table = tx
                .open_table(redb::TableDefinition::<&str, &[u8]>::new("Account"))
                .unwrap();
            table.insert("alice", &[255_u8][..]).unwrap();
        }
        tx.commit().unwrap();
    }
    let store = Store::open(path).unwrap();
    assert!(
        store
            .upgrade(
                &generated::UpgradePlan { target: 2 },
                &mut FaultInjection {
                    fail_after: None,
                    writes: 0
                }
            )
            .is_err()
    );
    assert_eq!(store.version().unwrap(), 0);
}

#[test]
fn tool_publishes_memory_manifest_and_rust_together() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = Bundle {
        root: directory.path().join("schema"),
    };
    let old = bundle
        .initialize(include_str!("../examples/memory.ethos").to_owned())
        .unwrap();
    let command = format!(
        "Edit.{{ {} AddField.{{ Account {{ Limit Integer }} Integer.100 }} }}",
        bundle.root.display()
    );
    let result = Command::new(env!("CARGO_BIN_EXE_experiment-upgrade"))
        .arg(command)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let current = bundle.current().unwrap();
    assert_ne!(old, current);
    assert!(
        fs::read_to_string(old.join("memory.ethos"))
            .unwrap()
            .contains("Balance.Integer")
    );
    let manifest: Manifest = fs::read_to_string(current.join("manifest.datom"))
        .unwrap()
        .as_str()
        .read_datom()
        .unwrap();
    assert_eq!(manifest.steps.len(), 1);
    assert_eq!(
        fs::read_to_string(current.join("generated.rs")).unwrap(),
        manifest.generate_rust().unwrap()
    );
    assert_eq!(
        fs::read_to_string(current.join("memory.ethos")).unwrap(),
        manifest.schemas().unwrap().last().unwrap().memory_text()
    );
    let bad = Edit::RemoveField {
        record: "Account".into(),
        field: "Name".into(),
    };
    assert!(bundle.apply(bad.clone()).is_err());
    assert_eq!(bundle.current().unwrap(), current);
    fs::write(current.join("memory.ethos"), "hand edit").unwrap();
    assert!(
        bundle
            .apply(bad)
            .unwrap_err()
            .to_string()
            .contains("outside the manifest")
    );
    assert_eq!(bundle.current().unwrap(), current);
    assert!(bundle.initialize(String::new()).is_err());
}

#[test]
fn edit_validation_refuses_unknowns_collisions_defaults_and_noncontiguous_history() {
    let manifest = include_str!("../examples/manifest.datom").manifest();
    let source = manifest.base_memory.read_memory().unwrap();
    for edit in [
        Edit::AddField {
            record: "Account".into(),
            field: Field {
                name: "Limit".into(),
                kind: FieldType::Integer,
            },
            default: DefaultValue::Text("wrong".into()),
        },
        Edit::RenameField {
            record: "Account".into(),
            from: "Balance".into(),
            to: "Name".into(),
        },
        Edit::RemoveField {
            record: "Account".into(),
            field: "Unknown".into(),
        },
        Edit::RenameRecord {
            from: "Account".into(),
            to: "Status".into(),
        },
        Edit::RemoveVariant {
            enumeration: "Status".into(),
            variant: "Frozen".into(),
            replacement: "Unknown".into(),
        },
        Edit::AddVariant {
            enumeration: "Status".into(),
            variant: "Open".into(),
        },
    ] {
        assert!(edit.apply(&source).is_err(), "accepted {edit:?}");
    }
    let mut gap = manifest;
    gap.steps[0].version = 2;
    assert!(gap.generate_rust().is_err());
}
