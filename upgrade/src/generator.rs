use crate::model::*;
use anyhow::Result;
use std::fmt::Write;

pub trait RustGenerating {
    fn generate_rust(&self) -> Result<String>;
}
pub trait FieldRust {
    fn rust_type(&self) -> String;
}
pub trait DefaultRust {
    fn rust_expression(&self, kind: &FieldType, version: usize) -> String;
}

impl FieldRust for FieldType {
    fn rust_type(&self) -> String {
        match self {
            Self::Text => "String".into(),
            Self::Integer => "i64".into(),
            Self::Boolean => "bool".into(),
            Self::Enumeration(name) => name.clone(),
        }
    }
}
impl DefaultRust for DefaultValue {
    fn rust_expression(&self, kind: &FieldType, version: usize) -> String {
        match self {
            Self::Text(text) => format!("String::from({text:?})"),
            Self::Integer(value) => format!("{value}_i64"),
            Self::Boolean(value) => value.to_string(),
            Self::Variant(variant) => format!("v{version}::{}::{variant}", kind.rust_type()),
        }
    }
}
impl RustGenerating for Manifest {
    fn generate_rust(&self) -> Result<String> {
        let schemas = self.schemas()?;
        let mut output = String::from(
            "// Generated from the edit manifest. Historical types are retained.\nuse experiment_upgrade::{UpgradeInto, TableMigrating, Migrating, MigrationObserver};\n",
        );
        for (version, schema) in schemas.iter().enumerate() {
            writeln!(output, "pub mod v{version} {{")?;
            for enumeration in &schema.enumerations {
                writeln!(
                    output,
                    "#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]\npub enum {} {{ {} }}",
                    enumeration.name,
                    enumeration.variants.join(", ")
                )?;
            }
            for record in &schema.records {
                writeln!(
                    output,
                    "#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]\npub struct {} {{",
                    record.name
                )?;
                for field in &record.fields {
                    writeln!(
                        output,
                        "pub {}: {},",
                        field.name.rust_field(),
                        field.kind.rust_type()
                    )?;
                }
                writeln!(output, "}}")?;
                writeln!(
                    output,
                    "impl experiment_upgrade::ArchiveRecord for {} {{\nconst TABLE: &'static str = {:?};\nfn identity(&self) -> &str {{ &self.{} }}\nfn encode(&self) -> anyhow::Result<Vec<u8>> {{ Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.to_vec()) }}\nfn decode(bytes: &[u8]) -> anyhow::Result<Self> {{ Ok(rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)?) }}\n}}",
                    record.name,
                    record.name,
                    record.fields[0].name.rust_field()
                )?;
            }
            writeln!(output, "}}")?;
        }
        for (index, step) in self.steps.iter().enumerate() {
            let old = index;
            let new = index + 1;
            let edit = &step.edits[0];
            for enumeration in &schemas[old].enumerations {
                writeln!(
                    output,
                    "impl UpgradeInto<v{new}::{}> for v{old}::{} {{\nfn upgrade(self) -> anyhow::Result<v{new}::{}> {{ Ok(match self {{",
                    enumeration.name, enumeration.name, enumeration.name
                )?;
                for variant in &enumeration.variants {
                    let replacement = match edit {
                        Edit::RemoveVariant {
                            enumeration: name,
                            variant: removed,
                            replacement,
                        } if name == &enumeration.name && variant == removed => replacement,
                        _ => variant,
                    };
                    writeln!(
                        output,
                        "v{old}::{}::{variant} => v{new}::{}::{replacement},",
                        enumeration.name, enumeration.name
                    )?;
                }
                writeln!(output, "}}) }} }}")?;
            }
            for record in &schemas[old].records {
                let target_name = match edit {
                    Edit::RenameRecord { from, to } if from == &record.name => to,
                    _ => &record.name,
                };
                let target = schemas[new]
                    .records
                    .iter()
                    .find(|r| &r.name == target_name)
                    .expect("checked edit preserves record");
                writeln!(
                    output,
                    "impl UpgradeInto<v{new}::{target_name}> for v{old}::{} {{\nfn upgrade(self) -> anyhow::Result<v{new}::{target_name}> {{\nlet old = self;",
                    record.name
                )?;
                if let Edit::Reshape {
                    record: name, body, ..
                } = edit
                    && name == &record.name
                {
                    writeln!(
                        output,
                        "let new: v{new}::{target_name} = {{ {body} }};\nOk(new)\n}} }}"
                    )?;
                    continue;
                }
                writeln!(output, "Ok(v{new}::{target_name} {{")?;
                for field in &target.fields {
                    let expression = match edit {
                        Edit::AddField {
                            record: name,
                            field: added,
                            default,
                        } if name == &record.name && field.name == added.name => {
                            default.rust_expression(&field.kind, new)
                        }
                        Edit::RenameField {
                            record: name,
                            from,
                            to,
                        } if name == &record.name && &field.name == to => {
                            format!("old.{}", from.rust_field())
                        }
                        _ => match &field.kind {
                            FieldType::Enumeration(_) => {
                                format!("old.{}.upgrade()?", field.name.rust_field())
                            }
                            _ => format!("old.{}", field.name.rust_field()),
                        },
                    };
                    writeln!(output, "{}: {expression},", field.name.rust_field())?;
                }
                writeln!(output, "}})\n}} }}")?;
            }
        }
        writeln!(
            output,
            "/// Selects a compiled target, allowing a store to stop at an intermediate version.\npub struct UpgradePlan {{ pub target: u32 }}\nimpl Migrating for UpgradePlan {{\nfn target(&self) -> u32 {{ self.target }}\nfn migrate(&self, transaction: &redb::WriteTransaction, from: u32, observer: &mut dyn MigrationObserver) -> anyhow::Result<()> {{\nanyhow::ensure!(self.target <= {}, \"target exceeds compiled manifest\");\nanyhow::ensure!(from <= self.target, \"store is newer than target\");\nfor version in from..self.target {{ match version {{",
            self.steps.len()
        )?;
        for (index, schema) in schemas.iter().take(self.steps.len()).enumerate() {
            writeln!(output, "{index} => {{")?;
            let new = index + 1;
            for record in &schema.records {
                let target = match &self.steps[index].edits[0] {
                    Edit::RenameRecord { from, to } if from == &record.name => to,
                    _ => &record.name,
                };
                writeln!(
                    output,
                    "transaction.migrate_table::<v{index}::{}, v{new}::{target}>(observer)?;",
                    record.name
                )?;
            }
            writeln!(output, "}}")?;
        }
        writeln!(
            output,
            "_ => anyhow::bail!(\"unknown source version {{version}}\"),\n}} }}\nOk(())\n}} }}"
        )?;
        Ok(output)
    }
}
