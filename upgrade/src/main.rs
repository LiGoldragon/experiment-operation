use anyhow::{Result, ensure};
use datom_codec::{Composing, Datomizable};
use experiment_upgrade::{Bundle, BundleEditing, DatomReading, Edit, Manifest, RustGenerating};
use std::{env, fs, path::PathBuf};

#[derive(Composing, Datomizable)]
enum Command {
    Initialize {
        directory: String,
        memory_file: String,
    },
    Edit {
        directory: String,
        edit: Edit,
    },
    Generate {
        manifest_file: String,
        rust_file: String,
    },
}
trait Executing {
    fn execute(self) -> Result<()>;
}
impl Executing for Command {
    fn execute(self) -> Result<()> {
        match self {
            Self::Initialize {
                directory,
                memory_file,
            } => {
                Bundle {
                    root: PathBuf::from(directory),
                }
                .initialize(fs::read_to_string(memory_file)?)?;
            }
            Self::Edit { directory, edit } => {
                Bundle {
                    root: PathBuf::from(directory),
                }
                .apply(edit)?;
            }
            Self::Generate {
                manifest_file,
                rust_file,
            } => {
                let manifest: Manifest =
                    fs::read_to_string(manifest_file)?.as_str().read_datom()?;
                fs::write(rust_file, manifest.generate_rust()?)?;
            }
        }
        Ok(())
    }
}
fn main() -> Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        arguments.len() == 1,
        "takes one datom: Initialize.{{ directory memory_file }}, Edit.{{ directory edit }}, or Generate.{{ manifest_file rust_file }}"
    );
    arguments[0].as_str().read_datom::<Command>()?.execute()
}
