//! Edit-chain semantics shared by the pass-three generator. All runtime source
//! is emitted separately; this compiler runs only while building the Nexus.
#[path = "../schema/edit.rs"]
pub mod edit;
use crate::nexus::{NamedEntry, ReadingSections};
use anyhow::{Result, bail, ensure};
pub use edit::*;
use ethos_zero::{
    Actualizing, File, Generating, Position, Potential, Printable, Reference, TypeDeclaration,
    Variant,
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
pub trait MemorySchema {
    fn memory(&self) -> Result<File>;
    fn fingerprint(&self) -> Result<String>;
}
pub trait DeclarationName {
    fn name(&self) -> &str;
}
pub trait FieldDescription {
    fn reference(&self) -> Result<&Reference>;
    fn field_type_name(&self) -> String;
}
pub trait EditApplying {
    fn apply(&self, schema: &File) -> Result<File>;
}
pub trait ConversionExpression {
    fn needs_conversion(&self) -> bool;
    fn conversion_result(&self, value: &str) -> Result<String>;
    fn conversion(&self, value: &str) -> Result<String>;
}
pub trait FieldNames {
    fn names(&self) -> Result<Vec<String>>;
}
pub trait SnakeName {
    fn snake(&self) -> String;
}
pub trait ReferenceFieldName {
    fn base(&self) -> String;
}
pub trait ChainReading {
    fn schemas(&self) -> Result<Vec<File>>;
}
impl MemorySchema for str {
    fn memory(&self) -> Result<File> {
        let sections = self.root_sections("Memory")?;
        let projection = match sections.sections.len() {
            2 => self.to_owned(),
            5 => format!(
                "Memory {} [ {} {} ]",
                sections.sections[0],
                sections.entries[1].join(" "),
                sections.entries[4].join(" ")
            ),
            count => bail!(
                "Memory requires two storage sections or five complete sections, found {count}"
            ),
        };
        let f = Potential::<File>::from(projection)
            .actualize()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        ensure!(matches!(f, File::Memory(_)), "expected Memory");
        Ok(f)
    }
    fn fingerprint(&self) -> Result<String> {
        let canonical = self.memory()?.print();
        Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
    }
}
impl DeclarationName for TypeDeclaration {
    fn name(&self) -> &str {
        match self {
            Self::Struct(i, _) | Self::Enum(i, _) | Self::Alias(i, _) => i.name.as_ref(),
        }
    }
}
impl FieldDescription for Position {
    fn reference(&self) -> Result<&Reference> {
        match self {
            Self::Referenced(r) => Ok(r),
            _ => bail!("inline fields require a named declaration in this compiler"),
        }
    }
    fn field_type_name(&self) -> String {
        match self {
            Self::Referenced(r) => r.name.as_ref().to_owned(),
            Self::Declared(d) => d.name().to_owned(),
        }
    }
}
impl SnakeName for str {
    fn snake(&self) -> String {
        let chars: Vec<char> = self.chars().collect();
        let mut out = String::new();
        for (i, c) in chars.iter().enumerate() {
            if c.is_uppercase()
                && i > 0
                && (chars[i - 1].is_lowercase()
                    || chars.get(i + 1).is_some_and(|c| c.is_lowercase()))
            {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        }
        out
    }
}
impl ReferenceFieldName for Reference {
    fn base(&self) -> String {
        let mut parts = self.arguments.iter().map(|r| r.base()).collect::<Vec<_>>();
        parts.push(self.name.as_ref().snake());
        parts.join("_")
    }
}
impl FieldNames for [Position] {
    fn names(&self) -> Result<Vec<String>> {
        let bases = self
            .iter()
            .map(|p| p.reference().map(|r| r.base()))
            .collect::<Result<Vec<_>>>()?;
        let mut names = Vec::new();
        for (index, base) in bases.iter().enumerate() {
            let count = bases.iter().filter(|b| *b == base).count();
            if count == 1 {
                names.push(format!("r#{base}"));
            } else {
                let occurrence = bases[..index].iter().filter(|b| *b == base).count();
                let ordinal = match occurrence {
                    0 => "first",
                    1 => "second",
                    2 => "third",
                    3 => "fourth",
                    _ => bail!("more than four repeated field types unsupported"),
                };
                names.push(format!("r#{ordinal}_{base}"));
            }
        }
        Ok(names)
    }
}
impl EditApplying for Edit {
    fn apply(&self, schema: &File) -> Result<File> {
        let mut f = schema.clone();
        let File::Memory(memory) = &mut f else {
            bail!("expected Memory")
        };
        match self {
            Edit::AddField(e) => {
                let fragment = format!("Memory [] [ Probe.{{ {} }} ]", e.field_body.0).memory()?;
                let File::Memory(part) = fragment else {
                    unreachable!()
                };
                let TypeDeclaration::Struct(_, fields) = &part.types[0] else {
                    unreachable!()
                };
                ensure!(
                    fields.len() == 1 && fields[0].field_type_name() == e.field_name.0,
                    "field name differs from field body"
                );
                let target = memory
                    .types
                    .iter_mut()
                    .find(|d| d.name() == e.record_name.0)
                    .ok_or_else(|| anyhow::anyhow!("unknown record"))?;
                let TypeDeclaration::Struct(_, positions) = target else {
                    bail!("not a record")
                };
                ensure!(
                    !positions
                        .iter()
                        .any(|p| p.field_type_name() == e.field_name.0),
                    "duplicate field"
                );
                positions.push(fields[0].clone());
            }
            Edit::RemoveField(e) => {
                let target = memory
                    .types
                    .iter_mut()
                    .find(|d| d.name() == e.record_name.0)
                    .ok_or_else(|| anyhow::anyhow!("unknown record"))?;
                let TypeDeclaration::Struct(_, positions) = target else {
                    bail!("not a record")
                };
                let index = positions
                    .iter()
                    .position(|p| p.field_type_name() == e.field_name.0)
                    .ok_or_else(|| anyhow::anyhow!("unknown field"))?;
                ensure!(index != 0, "identity removal refused");
                positions.remove(index);
            }
            Edit::AddVariant(e) => {
                let fragment = format!(
                    "Memory [] [ Probe.[ {}{} Other ] ]",
                    e.variant_name.0, e.field_body.0
                )
                .memory()?;
                let File::Memory(part) = fragment else {
                    unreachable!()
                };
                let TypeDeclaration::Enum(_, variants) = &part.types[0] else {
                    unreachable!()
                };
                let target = memory
                    .types
                    .iter_mut()
                    .find(|d| d.name() == e.record_name.0)
                    .ok_or_else(|| anyhow::anyhow!("unknown enum"))?;
                let TypeDeclaration::Enum(_, target) = target else {
                    bail!("not an enum")
                };
                target.push(variants[0].clone());
            }
            Edit::RemoveVariant(e) => {
                let target = memory
                    .types
                    .iter_mut()
                    .find(|d| d.name() == e.record_name.0)
                    .ok_or_else(|| anyhow::anyhow!("unknown enum"))?;
                let TypeDeclaration::Enum(_, variants) = target else {
                    bail!("not an enum")
                };
                ensure!(
                    variants.iter().any(
                        |v| matches!(v,Variant::Bare(n) if n.as_ref()==e.replacement_variant_name.0)
                    ),
                    "replacement must be a surviving unit variant"
                );
                let index = variants
                    .iter()
                    .position(|v| match v {
                        Variant::Bare(n)
                        | Variant::Typed(n, _)
                        | Variant::Struct(n, _)
                        | Variant::Enum(n, _) => n.as_ref() == e.variant_name.0,
                    })
                    .ok_or_else(|| anyhow::anyhow!("unknown variant"))?;
                ensure!(
                    e.variant_name.0 != e.replacement_variant_name.0,
                    "replacement removed"
                );
                variants.remove(index);
            }
            Edit::Reshape(e) => {
                let replacement =
                    format!("Memory [] [ {}.{{ {} }} ]", e.record_name.0, e.field_body.0)
                        .memory()?;
                let File::Memory(part) = replacement else {
                    unreachable!()
                };
                let target = memory
                    .types
                    .iter_mut()
                    .find(|d| d.name() == e.record_name.0)
                    .ok_or_else(|| anyhow::anyhow!("unknown record"))?;
                *target = part.types[0].clone();
            }
            Edit::RenameField(_) | Edit::RenameRecord(_) => bail!(
                "rename edits are declared but not implemented in the widened compiler; use Reshape with an explicit conversion"
            ),
        }
        // Whole-file checking after the edit catches dangling references and collisions.
        f.print().memory()
    }
}
impl ConversionExpression for Reference {
    fn needs_conversion(&self) -> bool {
        match self.name.as_ref() {
            "String" | "Integer" | "Boolean" | "Decimal" => false,
            "Vector" | "Option" => self.arguments[0].needs_conversion(),
            _ => true,
        }
    }
    fn conversion_result(&self, value: &str) -> Result<String> {
        let expression = self.conversion(value)?;
        Ok(if let Some(expression) = expression.strip_suffix('?') {
            expression.to_owned()
        } else {
            format!("Ok({expression})")
        })
    }
    fn conversion(&self, value: &str) -> Result<String> {
        ensure!(self.source.is_none(), "imported conversions unsupported");
        if !self.needs_conversion() {
            return Ok(value.to_owned());
        }
        Ok(match self.name.as_ref() {
            "Vector" => format!(
                "{value}.into_iter().map(|item| {}).collect::<std::result::Result<Vec<_>,operation_scope::Failure>>()?",
                self.arguments[0].conversion_result("item")?
            ),
            "Option" => format!(
                "{value}.map(|item| -> std::result::Result<_,operation_scope::Failure> {{ {} }}).transpose()?",
                self.arguments[0].conversion_result("item")?
            ),
            _ => format!("{value}.upgrade()?"),
        })
    }
}
impl ChainReading for Manifest {
    fn schemas(&self) -> Result<Vec<File>> {
        let first = self.original_memory.0.memory()?;
        ensure!(
            first.print().fingerprint()? == self.fingerprint.0,
            "original fingerprint mismatch"
        );
        let mut schemas = vec![first];
        for step in &self.step_vector {
            ensure!(!step.edit_vector.is_empty(), "empty step");
            let mut next = schemas.last().expect("original").clone();
            for edit in &step.edit_vector {
                next = edit.apply(&next)?;
            }
            ensure!(
                next.print().fingerprint()? == step.fingerprint.0,
                "step fingerprint mismatch"
            );
            ensure!(
                !schemas.iter().any(|prior| prior.print() == next.print()),
                "manifest revisits an earlier storage schema"
            );
            schemas.push(next);
        }
        Ok(schemas)
    }
}

struct ConversionStep<'a> {
    source: &'a File,
    target: &'a File,
    edits: &'a [Edit],
    from: &'a str,
    to: &'a str,
}
trait DerivingConversions {
    fn conversions(&self) -> Result<String>;
}
impl DerivingConversions for ConversionStep<'_> {
    fn conversions(&self) -> Result<String> {
        let File::Memory(old) = self.source else {
            bail!("expected Memory")
        };
        let File::Memory(new) = self.target else {
            bail!("expected Memory")
        };
        let mut out = String::new();
        let from = self.from;
        let to = self.to;
        for declaration in &old.types {
            let name = declaration.name();
            let target = new
                .types
                .iter()
                .find(|d| d.name() == name)
                .ok_or_else(|| anyhow::anyhow!("missing target"))?;
            writeln!(
                out,
                "impl UpgradeInto<{to}::{name}> for {from}::{name} {{ fn upgrade(self)->std::result::Result<{to}::{name},operation_scope::Failure> {{ let old=self;"
            )?;
            if let Some(Edit::Reshape(e)) = self
                .edits
                .iter()
                .find(|e| matches!(e,Edit::Reshape(e) if e.record_name.0==name))
            {
                writeln!(out, "Ok({{ {} }})", e.default_body.0)?;
            } else {
                match (declaration, target) {
                    (TypeDeclaration::Alias(_, reference), TypeDeclaration::Alias(_, _)) => {
                        writeln!(out, "Ok({to}::{name}({}))", reference.conversion("old.0")?)?
                    }
                    (TypeDeclaration::Struct(_, fields), TypeDeclaration::Struct(_, targets)) => {
                        let old_names = fields.names()?;
                        let new_names = targets.names()?;
                        ensure!(fields.first() == targets.first(), "identity edits refused");
                        writeln!(out, "Ok({to}::{name} {{")?;
                        for (index, field) in targets.iter().enumerate() {
                            let expression = if let Some(source) = old_names
                                .iter()
                                .position(|old_name| old_name == &new_names[index])
                            {
                                fields[source]
                                    .reference()?
                                    .conversion(&format!("old.{}", old_names[source]))?
                            } else {
                                let Some(Edit::AddField(e))=self.edits.iter().find(|e|matches!(e,Edit::AddField(e) if e.record_name.0==name && e.field_name.0==field.field_type_name())) else {bail!("new field lacks a default")};
                                e.default_body.0.clone()
                            };
                            writeln!(out, "{}: {expression},", new_names[index])?;
                        }
                        writeln!(out, "}})")?;
                    }
                    (TypeDeclaration::Enum(_, variants), TypeDeclaration::Enum(_, _)) => {
                        writeln!(out, "Ok(match old {{")?;
                        for variant in variants {
                            let variant_name = match variant {
                                Variant::Bare(n)
                                | Variant::Typed(n, _)
                                | Variant::Struct(n, _)
                                | Variant::Enum(n, _) => n.as_ref(),
                            };
                            let removed = self.edits.iter().find_map(|e| {
                                if let Edit::RemoveVariant(e) = e {
                                    (e.record_name.0 == name && e.variant_name.0 == variant_name)
                                        .then_some(&e.replacement_variant_name.0)
                                } else {
                                    None
                                }
                            });
                            let replacement = removed.map(String::as_str).unwrap_or(variant_name);
                            match variant {
                                Variant::Bare(_) => writeln!(
                                    out,
                                    "{from}::{name}::{variant_name} => {to}::{name}::{replacement},"
                                )?,
                                Variant::Typed(_, reference) => {
                                    let body = if removed.is_some() {
                                        format!("{to}::{name}::{replacement}")
                                    } else {
                                        format!(
                                            "{to}::{name}::{replacement}({})",
                                            reference.conversion("value")?
                                        )
                                    };
                                    let binding = if removed.is_some() { "_" } else { "value" };
                                    writeln!(
                                        out,
                                        "{from}::{name}::{variant_name}({binding}) => {body},"
                                    )?;
                                }
                                _ => bail!(
                                    "inline enum payload declarations need named types in this compiler"
                                ),
                            }
                        }
                        writeln!(out, "}})")?;
                    }
                    _ => bail!("shape replacement needs Reshape"),
                }
            }
            writeln!(out, "}} }}")?;
        }
        Ok(out)
    }
}

/// A checked edit manifest, explicitly selected by a consumer's build policy.
pub struct MemoryHistory {
    pub manifest: Manifest,
}
pub struct GeneratedHistory {
    pub source: String,
    pub migration: String,
}
pub trait GeneratingHistory {
    fn generate_history(&self, current: &File, records: &[String]) -> Result<GeneratedHistory>;
}
impl GeneratingHistory for MemoryHistory {
    fn generate_history(&self, current: &File, records: &[String]) -> Result<GeneratedHistory> {
        let schemas = self.manifest.schemas()?;
        ensure!(
            schemas.last().expect("original").print() == current.print(),
            "manifest does not arrive at the authored Memory storage projection"
        );
        let mut source = String::from(
            "pub mod history {\npub trait UpgradeInto<T>{fn upgrade(self)->Result<T,operation_scope::Failure>;}\n",
        );
        for (i, schema) in schemas.iter().enumerate() {
            writeln!(
                source,
                "pub mod v{i} {{ {} }}",
                schema.generate().map_err(|e| anyhow::anyhow!("{e:?}"))?
            )?;
            let File::Memory(memory) = schema else {
                unreachable!()
            };
            for declaration in &memory.types {
                let name = declaration.name();
                writeln!(
                    source,
                    "impl operation_scope::Wire for v{i}::{name} {{fn encode(&self)->Result<Vec<u8>,operation_scope::Failure>{{rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|_|operation_scope::Failure::Wire)}}fn decode(bytes:&[u8])->Result<Self,operation_scope::Failure>{{rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|_|operation_scope::Failure::Wire)}}}}"
                )?;
                if records.iter().any(|r| r == name) {
                    let TypeDeclaration::Struct(_, fields) = declaration else {
                        bail!("record {name} is not a struct")
                    };
                    let names = fields.names()?;
                    let key = names
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("record {name} needs an identity"))?;
                    writeln!(
                        source,
                        "impl operation_scope::Memorizable for v{i}::{name} {{const TABLE:&'static str={name:?};fn identity(&self)->Vec<u8>{{rkyv::to_bytes::<rkyv::rancor::Error>(&self.{key}).expect(\"serializable identity\").to_vec()}}}}"
                    )?;
                }
            }
        }
        for (i, step) in self.manifest.step_vector.iter().enumerate() {
            source.push_str(
                &ConversionStep {
                    source: &schemas[i],
                    target: &schemas[i + 1],
                    edits: &step.edit_vector,
                    from: &format!("v{i}"),
                    to: &format!("v{}", i + 1),
                }
                .conversions()?,
            );
        }
        let last = schemas.len() - 1;
        source.push_str(
            &ConversionStep {
                source: &schemas[last],
                target: current,
                edits: &[],
                from: &format!("v{last}"),
                to: "super",
            }
            .conversions()?,
        );
        source.push_str("}\n");
        let mut migration = String::from(
            "use history::UpgradeInto; use operation_scope::{Memorizable,TransactionAccess};\n",
        );
        migration.push_str("match stored {\n");
        for (i, schema) in schemas.iter().enumerate() {
            let digest = Sha256::digest(schema.print().as_bytes());
            writeln!(migration, "{:?} => {{", digest.as_slice())?;
            for name in records {
                writeln!(
                    migration,
                    "for old in memory.records::<history::v{i}::{name}>()? {{ let identity=old.identity(); let value=old;"
                )?;
                for version in i + 1..schemas.len() {
                    writeln!(
                        migration,
                        "let value:history::v{version}::{name}=value.upgrade()?;"
                    )?;
                }
                writeln!(
                    migration,
                    "let current:{name}=value.upgrade()?; if identity!=current.identity() {{return Err(operation_scope::Failure::Wire.into())}} memory.forget::<history::v{i}::{name}>(&identity)?;memory.remember(current)?;records+=1;}}"
                )?;
            }
            migration.push_str("},\n");
        }
        migration.push_str("_=>return Err(operation_scope::Failure::Schema{stored,expected:MEMORY_FINGERPRINT}.into()),}\n");
        Ok(GeneratedHistory { source, migration })
    }
}

pub struct HistoryBundle {
    pub root: std::path::PathBuf,
}
pub trait EditingHistoryBundle {
    fn initialize(&self, memory: String) -> Result<std::path::PathBuf>;
    fn current(&self) -> Result<std::path::PathBuf>;
    fn append(&self, edits: Vec<Edit>) -> Result<std::path::PathBuf>;
}
pub trait ReconstructingMemory {
    fn complete_memory(&self) -> Result<String>;
}
impl ReconstructingMemory for Manifest {
    fn complete_memory(&self) -> Result<String> {
        let schemas = self.schemas()?;
        let current = schemas.last().expect("original");
        let original = self.original_memory.0.root_sections("Memory")?;
        match original.sections.len() {
            2 => Ok(current.print()),
            5 => {
                let File::Memory(current) = current else {
                    unreachable!()
                };
                let mut records = Vec::new();
                let mut types = Vec::new();
                let record_names = original.entries[1]
                    .iter()
                    .map(|entry| entry.entry_name())
                    .collect::<Result<Vec<_>>>()?;
                for declaration in &current.types {
                    if record_names.iter().any(|name| name == declaration.name()) {
                        records.push(declaration.clone());
                    } else {
                        types.push(declaration.clone());
                    }
                }
                let render = |types| {
                    File::Memory(ethos_zero::Memory {
                        imports: current.imports.clone(),
                        types,
                    })
                    .print()
                    .root_sections("Memory")
                    .map(|s| s.sections[1].clone())
                };
                Ok(format!(
                    "Memory\n{}\n{}\n{}\n{}\n{}\n",
                    original.sections[0],
                    render(records)?,
                    original.sections[2],
                    original.sections[3],
                    render(types)?
                ))
            }
            count => bail!("Memory requires two or five sections, found {count}"),
        }
    }
}
impl EditingHistoryBundle for HistoryBundle {
    fn initialize(&self, memory: String) -> Result<std::path::PathBuf> {
        std::fs::create_dir_all(&self.root)?;
        let lock = self.root.join("writer-lock");
        std::fs::create_dir(&lock)?;
        let result = (|| {
            ensure!(
                self.root.join("current").symlink_metadata().is_err(),
                "already initialized"
            );
            self.publish(&Manifest {
                fingerprint: Fingerprint(memory.as_str().fingerprint()?),
                original_memory: OriginalMemory(memory),
                step_vector: Vec::new(),
            })
        })();
        std::fs::remove_dir(lock)?;
        result
    }
    fn current(&self) -> Result<std::path::PathBuf> {
        Ok(std::fs::canonicalize(self.root.join("current"))?)
    }
    fn append(&self, edits: Vec<Edit>) -> Result<std::path::PathBuf> {
        use crate::DatomReading;
        let lock = self.root.join("writer-lock");
        std::fs::create_dir(&lock)?;
        let result = (|| {
            let current = self.current()?;
            let mut manifest: Manifest = std::fs::read_to_string(current.join("manifest.datom"))?
                .as_str()
                .read_datom()?;
            ensure!(
                std::fs::read_to_string(current.join("memory.ethos"))?
                    == manifest.complete_memory()?,
                "Memory changed outside tool"
            );
            let schemas = manifest.schemas()?;
            let mut next = schemas.last().expect("original").clone();
            ensure!(!edits.is_empty(), "empty step");
            for edit in &edits {
                next = edit.apply(&next)?;
            }
            manifest.step_vector.push(Step {
                fingerprint: Fingerprint(next.print().fingerprint()?),
                edit_vector: edits,
            });
            self.publish(&manifest)
        })();
        std::fs::remove_dir(lock)?;
        result
    }
}
trait PublishingHistory {
    fn publish(&self, manifest: &Manifest) -> Result<std::path::PathBuf>;
}
impl PublishingHistory for HistoryBundle {
    fn publish(&self, manifest: &Manifest) -> Result<std::path::PathBuf> {
        use crate::DatomWriting;
        use std::{fs, io::Write};
        let memory = manifest.complete_memory()?;
        let temporary = tempfile::Builder::new()
            .prefix("generation-")
            .tempdir_in(&self.root)?;
        for (name, contents) in [
            ("manifest.datom", manifest.datom_text()),
            ("memory.ethos", memory),
        ] {
            let mut file = fs::File::create(temporary.path().join(name))?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }
        fs::File::open(temporary.path())?.sync_all()?;
        let generation = temporary.keep();
        let publication = tempfile::Builder::new()
            .prefix("publication-")
            .tempdir_in(&self.root)?;
        let link = publication.path().join("current");
        std::os::unix::fs::symlink(generation.file_name().expect("generation"), &link)?;
        fs::rename(link, self.root.join("current"))?;
        fs::File::open(&self.root)?.sync_all()?;
        Ok(generation)
    }
}
