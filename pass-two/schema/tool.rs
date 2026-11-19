//! A bounded edit compiler; Ethos owns parsing, canonical print and Rust type generation.
use super::edit::*;
use anyhow::{Result, bail, ensure};
use ethos_zero::{
    Actualizing, File, Generating, Position, Potential, Printable, Reference, TypeDeclaration,
    Variant,
};
use experiment_upgrade::{DatomReading, DatomWriting};
use sha2::{Digest, Sha256};
use std::{fmt::Write, fs, path::PathBuf};

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
pub trait ChainDeriving {
    fn schemas(&self) -> Result<Vec<File>>;
    fn derive(&self) -> Result<String>;
}
pub trait ConversionExpression {
    fn needs_conversion(&self) -> bool;
    fn conversion_result(&self, value: &str) -> Result<String>;
    fn conversion(&self, value: &str) -> Result<String>;
}
pub trait FieldNames {
    fn names(&self) -> Result<Vec<String>>;
}
pub struct Bundle {
    pub root: PathBuf,
}
pub trait BundleEditing {
    fn initialize(&self, memory: String) -> Result<PathBuf>;
    fn current(&self) -> Result<PathBuf>;
    fn append(&self, edits: Vec<Edit>) -> Result<PathBuf>;
    fn publish(&self, manifest: &Manifest) -> Result<PathBuf>;
}
impl MemorySchema for str {
    fn memory(&self) -> Result<File> {
        let f = Potential::<File>::from(self)
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
pub trait SnakeName {
    fn snake(&self) -> String;
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
pub trait ReferenceFieldName {
    fn base(&self) -> String;
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
                "{value}.into_iter().map(|item| {}).collect::<anyhow::Result<Vec<_>>>()?",
                self.arguments[0].conversion_result("item")?
            ),
            "Option" => format!(
                "{value}.map(|item| -> anyhow::Result<_> {{ {} }}).transpose()?",
                self.arguments[0].conversion_result("item")?
            ),
            _ => format!("{value}.upgrade()?"),
        })
    }
}
impl ChainDeriving for Manifest {
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
            schemas.push(next);
        }
        Ok(schemas)
    }
    fn derive(&self) -> Result<String> {
        let schemas = self.schemas()?;
        let mut out = String::from(
            "// Generated from the typed edit chain.\nuse super::{UpgradeInto,TableMigrating,MigrationObserver};\n",
        );
        out.push_str("#[derive(Debug,Clone)]\npub struct Upgrade { pub from:Vec<u8>, pub to:Vec<u8>, pub fail_after:Option<usize> }\nimpl crate::Changeable for Upgrade {\n    type Yield=crate::trace::UpgradeYield;\n    type Refusal=crate::ledger::Failure;\n    fn change(self,memory:&mut crate::memory::Transaction)->Result<Self::Yield,Self::Refusal> {\n        super::UpgradeApplying::apply(self,memory)\n    }\n}\n");
        let mut stamps = Vec::new();
        for (i, schema) in schemas.iter().enumerate() {
            stamps.push(schema.print().fingerprint()?);
            let generated = schema.generate().map_err(|e| anyhow::anyhow!("{e:?}"))?;
            writeln!(out, "pub mod v{i} {{\n{generated}\n}}")?;
            let File::Memory(m) = schema else {
                unreachable!()
            };
            for d in &m.types {
                if let TypeDeclaration::Struct(_, fields) = d {
                    let names = fields.names()?;
                    ensure!(!names.is_empty(), "empty records unsupported");
                    ensure!(
                        matches!(
                            fields[0].reference()?.name.as_ref(),
                            "String" | "Integer" | "Boolean" | "Decimal"
                        ),
                        "the first field must be a primitive identity"
                    );
                    let key = if fields[0].reference()?.name.as_ref() == "String" {
                        format!("self.{}.as_bytes().to_vec()", names[0])
                    } else {
                        format!(
                            "rkyv::to_bytes::<rkyv::rancor::Error>(&self.{}).expect(\"scalar identity\").to_vec()",
                            names[0]
                        )
                    };
                    writeln!(out,"impl crate::memory::Memorizable for v{i}::{} {{
const TABLE: &'static str = {:?};
fn identity(&self)->Vec<u8> {{ {key} }}
fn encode(&self)->Result<Vec<u8>,crate::ledger::Failure> {{ rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|v|v.to_vec()).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }}
fn decode(bytes:&[u8])->Result<Self,crate::ledger::Failure> {{ rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|e|crate::ledger::Failure::Storage(e.to_string())) }}
}}",d.name(),d.name().snake())?;
                }
            }
        }
        writeln!(out, "pub const FINGERPRINTS: &[&str] = &{stamps:?};")?;
        for (i, step) in self.step_vector.iter().enumerate() {
            let j = i + 1;
            let File::Memory(old) = &schemas[i] else {
                unreachable!()
            };
            let File::Memory(new) = &schemas[j] else {
                unreachable!()
            };
            for declaration in &old.types {
                let name = declaration.name();
                let target = new
                    .types
                    .iter()
                    .find(|d| d.name() == name)
                    .ok_or_else(|| anyhow::anyhow!("missing target"))?;
                writeln!(
                    out,
                    "impl UpgradeInto<v{j}::{name}> for v{i}::{name} {{ fn upgrade(self)->anyhow::Result<v{j}::{name}> {{ let old=self;"
                )?;
                if let Some(Edit::Reshape(e)) = step
                    .edit_vector
                    .iter()
                    .find(|e| matches!(e,Edit::Reshape(e) if e.record_name.0==name))
                {
                    writeln!(out, "Ok({{ {} }})", e.default_body.0)?;
                } else {
                    match (declaration, target) {
                        (TypeDeclaration::Alias(_, reference), TypeDeclaration::Alias(_, _)) => {
                            writeln!(out, "Ok(v{j}::{name}({}))", reference.conversion("old.0")?)?
                        }
                        (
                            TypeDeclaration::Struct(_, fields),
                            TypeDeclaration::Struct(_, targets),
                        ) => {
                            let old_names = fields.names()?;
                            let new_names = targets.names()?;
                            ensure!(fields[0] == targets[0], "identity edits refused");
                            writeln!(out, "Ok(v{j}::{name} {{")?;
                            for (index, field) in targets.iter().enumerate() {
                                let expression = if let Some(source) = old_names
                                    .iter()
                                    .position(|old_name| old_name == &new_names[index])
                                {
                                    fields[source]
                                        .reference()?
                                        .conversion(&format!("old.{}", old_names[source]))?
                                } else {
                                    let Some(Edit::AddField(e))=step.edit_vector.iter().find(|e|matches!(e,Edit::AddField(e) if e.record_name.0==name && e.field_name.0==field.field_type_name())) else {bail!("new field lacks a default")};
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
                                let removed = step.edit_vector.iter().find_map(|e| {
                                    if let Edit::RemoveVariant(e) = e {
                                        (e.record_name.0 == name
                                            && e.variant_name.0 == variant_name)
                                            .then_some(&e.replacement_variant_name.0)
                                    } else {
                                        None
                                    }
                                });
                                let replacement =
                                    removed.map(String::as_str).unwrap_or(variant_name);
                                match variant {
                                    Variant::Bare(_) => writeln!(
                                        out,
                                        "v{i}::{name}::{variant_name} => v{j}::{name}::{replacement},"
                                    )?,
                                    Variant::Typed(_, reference) => {
                                        let body = if removed.is_some() {
                                            format!("v{j}::{name}::{replacement}")
                                        } else {
                                            format!(
                                                "v{j}::{name}::{replacement}({})",
                                                reference.conversion("value")?
                                            )
                                        };
                                        let binding = if removed.is_some() { "_" } else { "value" };
                                        writeln!(
                                            out,
                                            "v{i}::{name}::{variant_name}({binding}) => {body},"
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
        }
        writeln!(
            out,
            "impl super::GeneratedMigrating for crate::memory::Transaction {{ fn migrate_generated(&mut self, from:usize, observer:&mut dyn MigrationObserver)->anyhow::Result<()> {{ let transaction=self; for step in from..{} {{ match step {{",
            self.step_vector.len()
        )?;
        for (i, schema) in schemas.iter().take(self.step_vector.len()).enumerate() {
            let j = i + 1;
            let File::Memory(m) = schema else {
                unreachable!()
            };
            writeln!(out, "{i} => {{")?;
            for d in &m.types {
                if matches!(d, TypeDeclaration::Struct(..)) {
                    writeln!(
                        out,
                        "transaction.migrate_table::<v{i}::{0},v{j}::{0}>(observer)?;",
                        d.name()
                    )?;
                }
            }
            writeln!(out, "}},")?;
        }
        writeln!(out, "_ => unreachable!(), }} }} Ok(()) }} }}")?;
        Ok(out)
    }
}
impl BundleEditing for Bundle {
    fn initialize(&self, memory: String) -> Result<PathBuf> {
        fs::create_dir_all(&self.root)?;
        let lock = self.root.join("writer-lock");
        fs::create_dir(&lock)?;
        let result = (|| {
            ensure!(
                self.root.join("current").symlink_metadata().is_err(),
                "already initialized"
            );
            let fingerprint = Fingerprint(memory.as_str().fingerprint()?);
            self.publish(&Manifest {
                original_memory: OriginalMemory(memory),
                fingerprint,
                step_vector: vec![],
            })
        })();
        fs::remove_dir(lock)?;
        result
    }
    fn current(&self) -> Result<PathBuf> {
        Ok(fs::canonicalize(self.root.join("current"))?)
    }
    fn append(&self, edits: Vec<Edit>) -> Result<PathBuf> {
        let lock = self.root.join("writer-lock");
        fs::create_dir(&lock)?;
        let result = (|| {
            let current = self.current()?;
            let mut manifest: Manifest = fs::read_to_string(current.join("manifest.datom"))?
                .as_str()
                .read_datom()?;
            let schemas = manifest.schemas()?;
            let schema = schemas.last().expect("original");
            ensure!(
                fs::read_to_string(current.join("memory.ethos"))? == schema.print(),
                "memory changed outside tool"
            );
            ensure!(
                fs::read_to_string(current.join("history.rs"))? == manifest.derive()?,
                "generated history changed outside tool"
            );
            let mut next = schema.clone();
            for edit in &edits {
                next = edit.apply(&next)?;
            }
            manifest.step_vector.push(Step {
                fingerprint: Fingerprint(next.print().fingerprint()?),
                edit_vector: edits,
            });
            self.publish(&manifest)
        })();
        fs::remove_dir(lock)?;
        result
    }
    fn publish(&self, manifest: &Manifest) -> Result<PathBuf> {
        let history = manifest.derive()?;
        let schemas = manifest.schemas()?;
        let temporary = tempfile::Builder::new()
            .prefix("generation-")
            .tempdir_in(&self.root)?;
        for (name, contents) in [
            ("manifest.datom", manifest.datom_text()),
            ("memory.ethos", schemas.last().expect("original").print()),
            ("history.rs", history),
        ] {
            use std::io::Write;
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
