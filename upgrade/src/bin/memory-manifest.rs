//! The manifest editor publishes Memory and its edit log by one symlink switch.
use experiment_upgrade::{
    DatomReading,
    memory_history::{Edit, EditingHistoryBundle, HistoryBundle},
};
fn main() -> anyhow::Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        arguments.len() == 3,
        "memory-manifest <init|edit> <bundle-directory> <memory-file|edits-datom>"
    );
    let bundle = HistoryBundle {
        root: arguments[1].clone().into(),
    };
    let generation = match arguments[0].as_str() {
        "init" => bundle.initialize(std::fs::read_to_string(&arguments[2])?)?,
        "edit" => bundle.append(arguments[2].as_str().read_datom::<Vec<Edit>>()?)?,
        _ => anyhow::bail!("expected init or edit"),
    };
    println!("{}", generation.display());
    Ok(())
}
