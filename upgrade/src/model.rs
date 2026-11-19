use anyhow::{Result, bail, ensure};
use datom_codec::{Composing, Datomizable};
use ethos_zero::{Actualizing, File, Position, Potential, TypeDeclaration, Variant};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub enum FieldType { Text, Integer, Boolean, Enumeration(String) }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub enum DefaultValue { Text(String), Integer(i64), Boolean(bool), Variant(String) }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub struct Field { pub name: String, pub kind: FieldType }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub struct Record { pub name: String, pub fields: Vec<Field> }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub struct Enumeration { pub name: String, pub variants: Vec<String> }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub struct Schema { pub records: Vec<Record>, pub enumerations: Vec<Enumeration> }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub enum Edit {
    AddField { record: String, field: Field, default: DefaultValue },
    RemoveField { record: String, field: String },
    RenameField { record: String, from: String, to: String },
    RenameRecord { from: String, to: String },
    AddVariant { enumeration: String, variant: String },
    RemoveVariant { enumeration: String, variant: String, replacement: String },
    Reshape { record: String, fields: Vec<Field>, body: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub struct Step { pub version: i64, pub edits: Vec<Edit> }
#[derive(Debug, Clone, PartialEq, Eq, Composing, Datomizable)]
pub struct Manifest { pub base_memory: String, pub steps: Vec<Step> }

/// A Memory source understood by this deliberately bounded trial.
pub trait MemoryReading { fn read_memory(&self) -> Result<Schema>; }
/// A checked schema: first field is an immutable text identity; enums are unit-only.
pub trait SchemaChecking { fn check(&self) -> Result<()>; fn memory_text(&self) -> String; }
/// Pure edit application, before any file or database changes.
pub trait Editing { fn apply(&self, schema: &Schema) -> Result<Schema>; }
pub trait HistoryReading { fn schemas(&self) -> Result<Vec<Schema>>; }
pub trait IdentifierChecking { fn check_identifier(&self) -> Result<()>; fn rust_field(&self) -> String; }

impl IdentifierChecking for str {
    fn check_identifier(&self) -> Result<()> {
        ensure!(self.starts_with(|c: char| c.is_ascii_uppercase()) && self.chars().all(|c| c.is_ascii_alphanumeric()), "expected PascalCase identifier: {self}");
        ensure!(self != "Self", "Self is reserved");
        Ok(())
    }
    fn rust_field(&self) -> String {
        let mut result = String::from("r#");
        for (index, c) in self.chars().enumerate() {
            if index != 0 && c.is_ascii_uppercase() { result.push('_'); }
            result.push(c.to_ascii_lowercase());
        }
        result
    }
}
impl MemoryReading for str {
    fn read_memory(&self) -> Result<Schema> {
        let file = Potential::<File>::from(self).actualize().map_err(|e| anyhow::anyhow!("Memory parse: {e:?}"))?;
        let File::Memory(memory) = file else { bail!("expected Memory root") };
        ensure!(memory.imports.is_empty(), "trial requires a self-contained Memory file");
        let mut schema = Schema { records: vec![], enumerations: vec![] };
        for declaration in memory.types {
            match declaration {
                TypeDeclaration::Struct(identity, positions) => {
                    ensure!(identity.constraints.is_empty(), "generic records are outside this trial");
                    let mut fields = vec![];
                    for position in positions {
                        let field = match position {
                            Position::Declared(TypeDeclaration::Alias(identity, reference)) => {
                                ensure!(identity.constraints.is_empty() && reference.source.is_none() && reference.arguments.is_empty(), "plain scalar field required");
                                let kind = match reference.name.as_ref() {
                                    "String" => FieldType::Text, "Integer" => FieldType::Integer, "Boolean" => FieldType::Boolean,
                                    _ => bail!("inline fields must be scalar"),
                                };
                                Field { name: identity.name.as_ref().to_owned(), kind }
                            }
                            Position::Referenced(reference) => {
                                ensure!(reference.source.is_none() && reference.arguments.is_empty(), "plain enum reference required");
                                Field { name: reference.name.as_ref().to_owned(), kind: FieldType::Enumeration(reference.name.as_ref().to_owned()) }
                            }
                            _ => bail!("nested fields are outside this trial"),
                        };
                        fields.push(field);
                    }
                    schema.records.push(Record { name: identity.name.as_ref().to_owned(), fields });
                }
                TypeDeclaration::Enum(identity, variants) => {
                    ensure!(identity.constraints.is_empty(), "generic enum unsupported");
                    let mut names = vec![];
                    for variant in variants {
                        let Variant::Bare(name) = variant else { bail!("only unit enum variants are supported") };
                        names.push(name.as_ref().to_owned());
                    }
                    schema.enumerations.push(Enumeration { name: identity.name.as_ref().to_owned(), variants: names });
                }
                _ => bail!("top-level scalar aliases are outside this trial; use inline scalar fields"),
            }
        }
        schema.check()?;
        Ok(schema)
    }
}
impl SchemaChecking for Schema {
    fn check(&self) -> Result<()> {
        ensure!(!self.records.is_empty(), "at least one record required");
        let mut names = BTreeSet::new();
        for enumeration in &self.enumerations {
            enumeration.name.check_identifier()?;
            ensure!(names.insert(&enumeration.name), "duplicate type");
            ensure!(enumeration.variants.len() >= 2, "an enum has at least two variants");
            let mut variants = BTreeSet::new();
            for name in &enumeration.variants { name.check_identifier()?; ensure!(variants.insert(name), "duplicate variant"); }
        }
        for record in &self.records {
            record.name.check_identifier()?;
            ensure!(names.insert(&record.name), "duplicate type");
            ensure!(record.fields.first().is_some_and(|f| f.kind == FieldType::Text), "first field must be text identity");
            let mut fields = BTreeSet::new();
            for field in &record.fields {
                field.name.check_identifier()?;
                ensure!(fields.insert(field.name.rust_field()), "duplicate field");
                if let FieldType::Enumeration(name) = &field.kind {
                    ensure!(self.enumerations.iter().any(|e| &e.name == name), "unknown enum {name}");
                    // Ethos references name the field by its type.
                    ensure!(&field.name == name, "enum field must be named by its type");
                }
            }
        }
        Ok(())
    }
    fn memory_text(&self) -> String {
        let mut declarations = vec![];
        for record in &self.records {
            let fields = record.fields.iter().map(|field| match &field.kind {
                FieldType::Text => format!("{}.String", field.name),
                FieldType::Integer => format!("{}.Integer", field.name),
                FieldType::Boolean => format!("{}.Boolean", field.name),
                FieldType::Enumeration(name) => name.clone(),
            }).collect::<Vec<_>>().join(" ");
            declarations.push(format!("{}.{{ {fields} }}", record.name));
        }
        for enumeration in &self.enumerations { declarations.push(format!("{}.[ {} ]", enumeration.name, enumeration.variants.join(" "))); }
        format!("Memory\n[]\n[ {} ]\n", declarations.join("\n  "))
    }
}
impl Editing for Edit {
    fn apply(&self, source: &Schema) -> Result<Schema> {
        let mut schema = source.clone();
        match self {
            Self::AddField { record, field, default } => {
                let valid = match (&field.kind, default) {
                    (FieldType::Text, DefaultValue::Text(_)) | (FieldType::Integer, DefaultValue::Integer(_)) | (FieldType::Boolean, DefaultValue::Boolean(_)) => true,
                    (FieldType::Enumeration(name), DefaultValue::Variant(variant)) => schema.enumerations.iter().any(|e| &e.name == name && e.variants.contains(variant)),
                    _ => false,
                };
                ensure!(valid, "default does not have the field's type");
                schema.records.iter_mut().find(|r| &r.name == record).ok_or_else(|| anyhow::anyhow!("unknown record {record}"))?.fields.push(field.clone());
            }
            Self::RemoveField { record, field } | Self::RenameField { record, from: field, .. } => {
                let target = schema.records.iter_mut().find(|r| &r.name == record).ok_or_else(|| anyhow::anyhow!("unknown record {record}"))?;
                let index = target.fields.iter().position(|f| &f.name == field).ok_or_else(|| anyhow::anyhow!("unknown field {field}"))?;
                ensure!(index != 0, "identity edits are outside this trial");
                match self {
                    Self::RemoveField { .. } => { target.fields.remove(index); }
                    Self::RenameField { to, .. } => { target.fields[index].name = to.clone(); }
                    _ => unreachable!(),
                }
            }
            Self::RenameRecord { from, to } => {
                schema.records.iter_mut().find(|r| &r.name == from).ok_or_else(|| anyhow::anyhow!("unknown record {from}"))?.name = to.clone();
            }
            Self::AddVariant { enumeration, variant } => {
                schema.enumerations.iter_mut().find(|e| &e.name == enumeration).ok_or_else(|| anyhow::anyhow!("unknown enum {enumeration}"))?.variants.push(variant.clone());
            }
            Self::RemoveVariant { enumeration, variant, replacement } => {
                let target = schema.enumerations.iter_mut().find(|e| &e.name == enumeration).ok_or_else(|| anyhow::anyhow!("unknown enum {enumeration}"))?;
                ensure!(variant != replacement && target.variants.contains(replacement), "replacement must be a surviving variant");
                let index = target.variants.iter().position(|v| v == variant).ok_or_else(|| anyhow::anyhow!("unknown variant {variant}"))?;
                target.variants.remove(index);
            }
            Self::Reshape { record, fields, body } => {
                ensure!(!body.trim().is_empty(), "Reshape requires a Rust expression using old");
                let target = schema.records.iter_mut().find(|r| &r.name == record).ok_or_else(|| anyhow::anyhow!("unknown record {record}"))?;
                ensure!(target.fields.first() == fields.first(), "Reshape must preserve the identity field");
                target.fields = fields.clone();
            }
        }
        schema.check()?;
        // Refuse a projection that our Ethos subset cannot faithfully read back.
        ensure!(schema.memory_text().read_memory()? == schema, "edit cannot be represented in this Memory subset");
        Ok(schema)
    }
}
impl HistoryReading for Manifest {
    fn schemas(&self) -> Result<Vec<Schema>> {
        let mut schemas = vec![self.base_memory.read_memory()?];
        for (index, step) in self.steps.iter().enumerate() {
            ensure!(step.version == i64::try_from(index)? + 1, "manifest versions must be contiguous");
            // One edit per published generation keeps each generated conversion one attempt.
            ensure!(step.edits.len() == 1, "this tool publishes one edit per step");
            schemas.push(step.edits[0].apply(schemas.last().expect("base schema"))?);
        }
        Ok(schemas)
    }
}
