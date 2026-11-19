use ethos_zero::{Actualizing, File, Generating, Potential};
use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-changed=schema/edit.ethos");
    println!("cargo:rerun-if-changed=schema/edit.rs");
    let source = fs::read_to_string("schema/edit.ethos").expect("edit ethos");
    let file = Potential::<File>::from(source)
        .actualize()
        .expect("valid edit ethos");
    let generated = file.generate().expect("generated edits");
    assert_eq!(
        generated,
        fs::read_to_string("schema/edit.rs").expect("committed generated edits"),
        "regenerate schema/edit.rs with ethos-zero"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("output"));
    fs::write(out.join("edit.rs"), generated).expect("write edits");
}
