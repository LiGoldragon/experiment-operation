use crate::{Edit, HistoryReading, Manifest, RustGenerating, SchemaChecking, Step};
use anyhow::{Result, ensure};
use datom_codec::{Actualizing, Budget, Composing, Datomizable, Potential};
use protos::{Protosizable, ReaderBudget, Textualizable};
use std::{fs, io::Write, path::PathBuf};

/// An immutable generation contains the manifest, its Memory projection and its Rust.
/// Publication changes one symlink. Readers resolve it once before opening its files.
pub struct Bundle {
    pub root: PathBuf,
}
pub trait BundleEditing {
    fn initialize(&self, memory: String) -> Result<PathBuf>;
    fn current(&self) -> Result<PathBuf>;
    fn apply(&self, edit: Edit) -> Result<PathBuf>;
    fn publish(&self, manifest: &Manifest) -> Result<PathBuf>;
}
pub trait DatomReading {
    fn read_datom<T: Composing>(&self) -> Result<T>;
}
pub trait DatomWriting {
    fn datom_text(&self) -> String;
}
impl DatomReading for str {
    fn read_datom<T: Composing>(&self) -> Result<T> {
        // Each node consumes at least one byte; budgets are derived from this input.
        let size = self.len().saturating_add(1);
        let bound = i64::try_from(size)?;
        let mut budget = Budget {
            remaining: bound,
            reader: ReaderBudget::try_from(size).map_err(|e| anyhow::anyhow!("budget: {e:?}"))?,
            depth: 0,
            maximum_depth: bound,
        };
        Potential::<T>::from(self)
            .actualize(&mut budget)
            .map_err(|e| anyhow::anyhow!("datom: {e:?}"))
    }
}
impl<T: Datomizable> DatomWriting for T {
    fn datom_text(&self) -> String {
        self.datomize(vec![]).protosize().textualize()
    }
}
impl BundleEditing for Bundle {
    fn initialize(&self, memory: String) -> Result<PathBuf> {
        fs::create_dir_all(&self.root)?;
        ensure!(
            !self.root.join("current").symlink_metadata().is_ok(),
            "bundle already initialized"
        );
        self.publish(&Manifest {
            base_memory: memory,
            steps: vec![],
        })
    }
    fn current(&self) -> Result<PathBuf> {
        Ok(fs::canonicalize(self.root.join("current"))?)
    }
    fn apply(&self, edit: Edit) -> Result<PathBuf> {
        // This is the tool's single-writer lock. A crashed lock is intentionally explicit.
        let lock = self.root.join("writer-lock");
        fs::create_dir(&lock)?;
        let result = (|| {
            let generation = self.current()?;
            let mut manifest: Manifest = fs::read_to_string(generation.join("manifest.datom"))?
                .as_str()
                .read_datom()?;
            let schemas = manifest.schemas()?;
            ensure!(
                fs::read_to_string(generation.join("memory.ethos"))?
                    == schemas.last().expect("base schema").memory_text(),
                "Memory file was edited outside the manifest tool"
            );
            ensure!(
                fs::read_to_string(generation.join("generated.rs"))? == manifest.generate_rust()?,
                "generated Rust differs from the manifest"
            );
            manifest.steps.push(Step {
                version: i64::try_from(manifest.steps.len())? + 1,
                edits: vec![edit],
            });
            self.publish(&manifest)
        })();
        fs::remove_dir(lock)?;
        result
    }
    fn publish(&self, manifest: &Manifest) -> Result<PathBuf> {
        let schemas = manifest.schemas()?;
        let rust = manifest.generate_rust()?;
        let directory = tempfile::Builder::new()
            .prefix("generation-")
            .tempdir_in(&self.root)?;
        for (name, contents) in [
            (
                "memory.ethos",
                schemas.last().expect("base schema").memory_text(),
            ),
            ("manifest.datom", manifest.datom_text()),
            ("generated.rs", rust),
        ] {
            let mut file = fs::File::create(directory.path().join(name))?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }
        fs::File::open(directory.path())?.sync_all()?;
        let generation = directory.keep();
        let link_directory = tempfile::Builder::new()
            .prefix("publication-")
            .tempdir_in(&self.root)?;
        let link = link_directory.path().join("current");
        std::os::unix::fs::symlink(generation.file_name().expect("generation name"), &link)?;
        // Both entries are on one filesystem. The target is relative to the final parent.
        fs::rename(&link, self.root.join("current"))?;
        fs::File::open(&self.root)?.sync_all()?;
        Ok(generation)
    }
}
