use experiment_upgrade::{GeneratingNexus, NexusSchema};
fn lock() -> NexusSchema {
    NexusSchema {
        signal: "Signal [] [ Lock Release ] [ Locked Released Refused ] []".into(),
        memory: "Memory [] [ HeldLock.{ LockName } ] [ Lock.LockName Release.LockName ] [ Locked Released Refused.[ Held Unknown ] ] [ LockName.String ]".into(),
        operation: "Operation [] [] [] []".into(),
        translation: "Translation [] [] [] []".into(),
    }
}
#[test]
fn generates_complete_lock_vocabulary_and_bindings() {
    let generated = lock().generate_nexus().unwrap();
    assert!(generated.types.contains("pub struct Lock"));
    assert!(generated.types.contains("pub lock_name: LockName"));
    assert!(
        generated
            .types
            .contains("Runtime(operation_scope::Failure)")
    );
    assert!(
        generated
            .bindings
            .contains("impl operation_scope::Memorizable for HeldLock")
    );
    assert!(
        generated
            .bindings
            .contains("impl operation_scope::Serving<NexusDomain> for Nexus")
    );
    assert!(generated.trace.contains("pub enum Object"));
    assert!(generated.trace.contains("pub enum Outcome"));
}
#[test]
fn memory_stamp_ignores_changes_and_signal_stamp_changes_with_contract() {
    let mut schema = lock();
    let before = schema.generate_nexus().unwrap().fingerprints;
    schema.memory = schema.memory.replace(
        "Lock.LockName Release.LockName",
        "Release.LockName Lock.LockName",
    );
    assert_eq!(before, schema.generate_nexus().unwrap().fingerprints);
    schema.signal = schema
        .signal
        .replace("Lock Release", "Release Lock")
        .replace("Locked Released", "Released Locked");
    let after = schema.generate_nexus().unwrap().fingerprints;
    assert_eq!(before.lines().next(), after.lines().next());
    assert_ne!(before, after);
}
#[test]
fn canonical_print_ignores_whitespace_comments_and_sweet_spelling() {
    let mut schema = lock();
    let before = schema.generate_nexus().unwrap().fingerprints;
    schema.signal =
        "; comment\nSignal.{ [] [ Lock\n Release ] [ Locked Released Refused ] [] }".into();
    assert_eq!(before, schema.generate_nexus().unwrap().fingerprints);
}
#[test]
fn imports_and_conflicting_declarations_are_explicit_refusals() {
    let mut schema = lock();
    schema.operation = "Operation [ foreign:Name ] [] [] []".into();
    assert!(
        schema
            .generate_nexus()
            .err()
            .unwrap()
            .to_string()
            .contains("imports are unsupported")
    );
    schema.operation = "Operation [] [] [] [ LockName.Integer ]".into();
    assert!(
        schema
            .generate_nexus()
            .err()
            .unwrap()
            .to_string()
            .contains("conflicting declaration")
    );
}

#[test]
fn history_is_derived_and_must_arrive_at_current_memory() {
    use ethos_zero::Printable;
    use experiment_upgrade::{GenerationPolicy, memory_history::*};
    let original = "Memory [] [ HeldLock.{ LockName } LockName.String ]";
    let edit = Edit::AddField(AddField_Data {
        record_name: RecordName("HeldLock".into()),
        field_name: FieldName("Integer".into()),
        field_body: FieldBody("Integer".into()),
        default_body: DefaultBody("0".into()),
    });
    let latest = edit.apply(&original.memory().unwrap()).unwrap().print();
    let manifest = Manifest {
        original_memory: OriginalMemory(original.into()),
        fingerprint: Fingerprint(original.fingerprint().unwrap()),
        step_vector: vec![Step {
            fingerprint: Fingerprint(latest.as_str().fingerprint().unwrap()),
            edit_vector: vec![edit],
        }],
    };
    let mut schema = lock();
    schema.memory = schema
        .memory
        .replace("HeldLock.{ LockName }", "HeldLock.{ LockName Integer }");
    let policy = GenerationPolicy {
        memory_history: Some(MemoryHistory { manifest }),
        ..Default::default()
    };
    let generated = schema.generate_nexus_with(&policy).unwrap();
    assert!(generated.bindings.contains("pub mod history"));
    assert!(
        generated
            .trace
            .contains("memory.records::<history::v0::HeldLock>")
    );
    assert!(
        generated
            .bindings
            .contains("impl UpgradeInto<v1::HeldLock> for v0::HeldLock")
    );
    schema.memory = schema.memory.replace(
        "HeldLock.{ LockName Integer }",
        "HeldLock.{ LockName Boolean }",
    );
    assert!(
        schema
            .generate_nexus_with(&policy)
            .err()
            .unwrap()
            .to_string()
            .contains("manifest does not arrive")
    );
}
#[test]
fn edit_types_are_exact_ethos_output() {
    use ethos_zero::{Actualizing, File, Generating, Potential};
    let file = Potential::<File>::from(include_str!("../schema/edit.ethos"))
        .actualize()
        .unwrap();
    assert_eq!(file.generate().unwrap(), include_str!("../schema/edit.rs"));
}
#[test]
fn repeated_identity_field_uses_ethos_ordinal() {
    let mut schema = lock();
    schema.memory = schema
        .memory
        .replace("HeldLock.{ LockName }", "HeldLock.{ LockName LockName }");
    let generated = schema.generate_nexus().unwrap();
    assert!(generated.bindings.contains("&self.r#first_lock_name"));
}

#[test]
fn complete_memory_and_manifest_publish_atomically_and_reject_tampering() {
    use experiment_upgrade::{DatomReading, memory_history::*};
    let directory = tempfile::tempdir().unwrap();
    let bundle = HistoryBundle {
        root: directory.path().to_owned(),
    };
    let source = lock().memory;
    let old = bundle.initialize(source).unwrap();
    let old_memory = std::fs::read_to_string(old.join("memory.ethos")).unwrap();
    let new = bundle
        .append(vec![Edit::AddField(AddField_Data {
            record_name: RecordName("HeldLock".into()),
            field_name: FieldName("Integer".into()),
            field_body: FieldBody("Integer".into()),
            default_body: DefaultBody("0".into()),
        })])
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(
        old_memory,
        std::fs::read_to_string(old.join("memory.ethos")).unwrap()
    );
    let manifest: Manifest = std::fs::read_to_string(new.join("manifest.datom"))
        .unwrap()
        .as_str()
        .read_datom()
        .unwrap();
    assert_eq!(manifest.step_vector.len(), 1);
    let memory = std::fs::read_to_string(new.join("memory.ethos")).unwrap();
    assert_eq!(memory, manifest.complete_memory().unwrap());
    assert!(memory.contains("Lock.LockName"));
    assert!(memory.contains("Integer"));
    std::fs::write(new.join("memory.ethos"), "tampered").unwrap();
    assert!(
        bundle
            .append(vec![Edit::RemoveField(RemoveField_Data {
                record_name: RecordName("HeldLock".into()),
                field_name: FieldName("Integer".into())
            })])
            .is_err()
    );
    assert_eq!(new, bundle.current().unwrap());
    assert!(!directory.path().join("writer-lock").exists());
}

#[test]
fn signal_stamp_uses_the_same_canonical_file_printer_as_memory() {
    use ethos_zero::{Actualizing, File, Potential, Printable};
    use sha2::{Digest, Sha256};
    let schema = lock();
    let signal = Potential::<File>::from(schema.signal.clone())
        .actualize()
        .unwrap();
    let digest = Sha256::digest(signal.print().as_bytes());
    assert!(
        schema
            .generate_nexus()
            .unwrap()
            .fingerprints
            .lines()
            .nth(1)
            .unwrap()
            .contains(&format!("{:?}", digest.as_slice()))
    );
}
