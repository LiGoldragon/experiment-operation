use experiment_upgrade::DatomReading;
use operation_ledger::upgrade::{Bundle, BundleEditing, Edit};
use std::{env, fs, path::PathBuf};
fn main() -> anyhow::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        args.len() == 3,
        "manifest <init|edit> <bundle-directory> <memory-file|edits-datom>"
    );
    let bundle = Bundle {
        root: PathBuf::from(&args[1]),
    };
    let result = match args[0].as_str() {
        "init" => bundle.initialize(fs::read_to_string(&args[2])?)?,
        "edit" => bundle.append(args[2].as_str().read_datom::<Vec<Edit>>()?)?,
        _ => anyhow::bail!("expected init or edit"),
    };
    println!("{}", result.display());
    Ok(())
}
