use experiment_upgrade::{
    GeneratedSource, GeneratingNexus, GenerationPolicy, NexusSchema, SubscriptionBinding,
};
use std::{fs, path::PathBuf};
fn main() {
    let read = |name| {
        let path = format!("ethos/{name}.ethos");
        println!("cargo:rerun-if-changed={path}");
        fs::read_to_string(path).unwrap()
    };
    let generated = NexusSchema {
        signal: read("signal"),
        memory: read("memory"),
        operation: read("operation"),
        translation: read("translation"),
    }
    .generate_nexus_with(&GenerationPolicy {
        resumables: vec!["Reservation".into()],
        fallible_attempts: vec!["Pay".into(), "Subscribe".into(), "Acknowledge".into()],
        held_attempts: vec!["AcquireLock".into()],
        subscriptions: vec![SubscriptionBinding {
            input: "Subscribe".into(),
            record: "Account".into(),
            output: "Subscribed".into(),
        }],
        ..Default::default()
    })
    .unwrap();
    let output = generated.rust_source();
    fs::write(
        PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("nexus.rs"),
        output,
    )
    .unwrap();
}
