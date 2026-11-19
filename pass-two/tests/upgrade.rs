#![cfg(feature = "datom")]
use experiment_upgrade::{DatomReading, DatomWriting};
use operation_ledger::{
    TransactionRunning,
    ledger::{Failure, Inspect},
    memory::{Memorizable, Store, StoreAccess, StoreOpening, TransactionAccess, TransactionCommit},
    scope::{ChangeExecuting, Scope, ScopeClosing, ScopeOpening, StubTranslator},
    transport::HolderMode,
    upgrade::{
        self, Bundle, BundleEditing, ChainDeriving, Manifest, MemorySchema, STAMP, UpgradePreparing,
    },
};
use redb::ReadableTable;
use std::fs;

trait Fixture {
    fn seed(&self);
}
impl Fixture for Store {
    fn seed(&self) {
        let mut tx = self.begin().unwrap();
        tx.write
            .open_table(STAMP)
            .unwrap()
            .insert("schema", upgrade::history::FINGERPRINTS[0])
            .unwrap();
        tx.remember(upgrade::history::v0::Account {
            string: "alice".into(),
            integer: 17,
        })
        .unwrap();
        tx.remember(upgrade::history::v0::Envelope {
            integer: 42,
            detail_vector: vec![upgrade::history::v0::Detail {
                string: "vector".into(),
                integer: 1,
            }],
            detail_option: Some(upgrade::history::v0::Detail {
                string: "option".into(),
                integer: 2,
            }),
            detail: upgrade::history::v0::Detail {
                string: "record".into(),
                integer: 3,
            },
            state: upgrade::history::v0::State::Active(upgrade::history::v0::Detail {
                string: "payload".into(),
                integer: 4,
            }),
        })
        .unwrap();
        tx.remember(upgrade::history::v0::Envelope {
            integer: 43,
            detail_vector: vec![],
            detail_option: None,
            detail: upgrade::history::v0::Detail {
                string: "other".into(),
                integer: 5,
            },
            state: upgrade::history::v0::State::Idle("idle payload".into()),
        })
        .unwrap();
        tx.commit().unwrap();
    }
}
#[test]
fn canonical_fingerprint_ignores_layout_but_retains_field_order() {
    let a = "Memory [] [ Account.{ String Integer } ]";
    let b = "Memory.{ [] [ Account.{ String Integer } ] }";
    assert_eq!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    assert_ne!(
        a.fingerprint().unwrap(),
        "Memory [] [ Account.{ Integer String } ]"
            .fingerprint()
            .unwrap()
    );
    // Same archive layout: both integer positions are valid, yet their meanings differ.
    assert_ne!(
        "Memory [] [ Pair.{ Left.Integer Right.Integer } ]"
            .fingerprint()
            .unwrap(),
        "Memory [] [ Pair.{ Right.Integer Left.Integer } ]"
            .fingerprint()
            .unwrap()
    );
}
#[test]
fn generated_history_and_edits_are_exact_products() {
    let manifest: Manifest = include_str!("../schema/manifest.datom")
        .read_datom()
        .unwrap();
    assert_eq!(
        manifest.derive().unwrap(),
        include_str!("../schema/history.rs")
    );
    assert_eq!(
        manifest.original_memory.0,
        include_str!("../schema/original.ethos")
    );
    use ethos_zero::Printable;
    assert_eq!(
        manifest.schemas().unwrap().last().unwrap().print(),
        include_str!("../schema/memory.ethos")
    );
    assert_eq!(manifest.step_vector.len(), 1);
    assert_eq!(manifest.step_vector[0].edit_vector.len(), 2);
    assert_eq!(manifest.schemas().unwrap().len(), 2);
}
#[test]
fn every_injected_write_rolls_back_records_and_stamp() {
    for fail_after in 1..=4 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.redb");
        let store = Store::open(path.clone()).unwrap();
        store.seed();
        let original = store
            .begin()
            .unwrap()
            .records::<upgrade::history::v0::Envelope>()
            .unwrap()[0]
            .encode()
            .unwrap();
        {
            let mut tx = store.begin().unwrap();
            let upgrade = tx.prepare_upgrade(Some(fail_after)).unwrap().unwrap();
            assert!(tx.run(upgrade).is_err());
            // Owner drops refused roots: no commit.
        }
        drop(store);
        let store = Store::open(path).unwrap();
        let tx = store.begin().unwrap();
        assert_eq!(
            tx.write
                .open_table(STAMP)
                .unwrap()
                .get("schema")
                .unwrap()
                .unwrap()
                .value(),
            upgrade::history::FINGERPRINTS[0]
        );
        assert_eq!(
            tx.records::<upgrade::history::v0::Envelope>().unwrap()[0]
                .encode()
                .unwrap(),
            original
        );
    }
}
#[tokio::test]
async fn widened_upgrade_is_first_change_on_the_ledger_machine() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.redb");
    {
        let store = Store::open(path.clone()).unwrap();
        store.seed();
    }
    let scope = Scope::open(path.clone(), HolderMode::Task, StubTranslator::default())
        .await
        .unwrap();
    assert_eq!(
        scope
            .change(Inspect {
                name: "alice".into()
            })
            .await
            .unwrap()
            .balance,
        17
    );
    #[cfg(feature = "trace")]
    {
        use operation_ledger::trace::{Event, Object, Outcome};
        let traces = scope.traces.history.lock().unwrap();
        assert!(matches!(
            &traces[0].event,
            Event::Entered(Object::Upgrade(_))
        ));
        assert!(matches!(&traces[1].event,Event::Yielded(Outcome::Upgrade(y)) if y.records==3));
        assert_eq!(traces[0].root, traces[1].root);
        assert!(traces[0].path.is_empty());
    }
    scope.shutdown().await;
    drop(scope);
    let store = Store::open(path).unwrap();
    let tx = store.begin().unwrap();
    assert!(tx.prepare_upgrade(None).unwrap().is_none());
    let records = tx.records::<upgrade::history::v1::Envelope>().unwrap();
    assert_eq!(records[0].integer, 42);
    assert_eq!(records[0].detail_vector[0].string, "vector");
    assert_eq!(records[0].detail_option.as_ref().unwrap().integer, 2);
    assert_eq!(records[0].detail.string, "record");
    assert!(
        matches!(&records[0].state,upgrade::history::v1::State::Active(detail) if detail.string=="payload")
    );
    assert!(records[0].boolean);
    assert_eq!(records[0].string, "upgraded");
}
#[tokio::test]
async fn unknown_and_reordered_schema_refuse_before_reading_archives() {
    for stamp in [
        "unrecognized".to_string(),
        "Memory [] [ Pair.{ Right.Integer Left.Integer } ]"
            .fingerprint()
            .unwrap(),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.redb");
        {
            let store = Store::open(path.clone()).unwrap();
            let tx = store.begin().unwrap();
            tx.write
                .open_table(STAMP)
                .unwrap()
                .insert("schema", stamp.as_str())
                .unwrap();
            tx.commit().unwrap();
        }
        let Err(Failure::Storage(message)) =
            Scope::open(path, HolderMode::Task, StubTranslator::default()).await
        else {
            panic!("wrong schema accepted")
        };
        assert!(message.contains(&stamp));
        assert!(message.contains(upgrade::history::FINGERPRINTS.last().unwrap()));
    }
}
#[test]
fn bundle_publication_keeps_memory_manifest_and_rust_together() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = Bundle {
        root: dir.path().join("bundle"),
    };
    let initial = bundle
        .initialize(include_str!("../schema/original.ethos").into())
        .unwrap();
    let edits = include_str!("../schema/edits.datom").read_datom().unwrap();
    let changed = bundle.append(edits).unwrap();
    assert_ne!(initial, changed);
    assert_eq!(bundle.current().unwrap(), changed);
    let manifest: Manifest = fs::read_to_string(changed.join("manifest.datom"))
        .unwrap()
        .as_str()
        .read_datom()
        .unwrap();
    assert_eq!(
        fs::read_to_string(changed.join("history.rs")).unwrap(),
        manifest.derive().unwrap()
    );
    assert_eq!(
        manifest.datom_text().read_datom::<Manifest>().unwrap(),
        manifest
    );
    let before = bundle.current().unwrap();
    fs::write(before.join("memory.ethos"), "tampered").unwrap();
    assert!(bundle.append(vec![]).is_err());
    assert_eq!(bundle.current().unwrap(), before);
}

#[test]
fn repeated_fields_and_empty_nested_shapes_keep_their_positions() {
    use upgrade::UpgradeInto;
    let old = upgrade::history::v0::Reservation {
        first_string: "reservation".into(),
        second_string: "alice".into(),
        integer: 7,
    };
    let new: upgrade::history::v1::Reservation = old.upgrade().unwrap();
    assert_eq!(new.first_string, "reservation");
    assert_eq!(new.second_string, "alice");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("memory.redb")).unwrap();
    store.seed();
    let mut tx = store.begin().unwrap();
    let upgrade = tx.prepare_upgrade(None).unwrap().unwrap();
    tx.run(upgrade).unwrap();
    tx.commit().unwrap();
    let tx = store.begin().unwrap();
    let values = tx.records::<upgrade::history::v1::Envelope>().unwrap();
    let empty = values.iter().find(|value| value.integer == 43).unwrap();
    assert!(empty.detail_vector.is_empty());
    assert!(empty.detail_option.is_none());
    assert!(matches!(&empty.state,upgrade::history::v1::State::Idle(text) if text=="idle payload"));
    assert!(
        tx.recall_key::<upgrade::history::v1::Envelope>(&empty.identity())
            .unwrap()
            .is_some()
    );
}
