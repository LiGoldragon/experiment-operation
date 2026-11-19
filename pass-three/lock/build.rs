use experiment_upgrade::memory_history::{Manifest, MemoryHistory};
use experiment_upgrade::{
    DatomReading, GeneratedSource, GeneratingNexus, GenerationPolicy, NexusSchema,
};
use std::{fs, path::PathBuf};
fn main() {
    let read = |name| {
        let path = format!("ethos/{name}.ethos");
        println!("cargo:rerun-if-changed={path}");
        fs::read_to_string(path).unwrap()
    };
    println!("cargo:rerun-if-changed=ethos/manifest.datom");
    let generated = NexusSchema {
        signal: read("signal"),
        memory: read("memory"),
        operation: read("operation"),
        translation: read("translation"),
    }
    .generate_nexus_with(&GenerationPolicy {
        memory_history: Some(MemoryHistory {
            manifest: fs::read_to_string("ethos/manifest.datom")
                .unwrap()
                .as_str()
                .read_datom::<Manifest>()
                .unwrap(),
        }),
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
