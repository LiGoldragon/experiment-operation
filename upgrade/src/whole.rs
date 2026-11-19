//! Whole attempts are checked as type-directed compositions before Rust emission.
use crate::nexus::{GenerationPolicy, NamedEntry, NexusSchema, ReadingSections, Sections};
use anyhow::{Result, bail, ensure};
use protos::{Enclosure, Protos, Protosizable};
use std::collections::BTreeMap;
use std::fmt::Write;

pub(crate) struct WholeForms {
    pub operation: String,
    pub memory: String,
    attempts: Vec<Whole>,
    changes: Vec<Whole>,
}
struct Whole {
    name: String,
    positions: Vec<Protos>,
    outcome: String,
}
struct Available {
    name: String,
    expression: String,
}
struct Composition<'a> {
    attempt: &'a str,
    definitions: &'a BTreeMap<String, String>,
    calls: &'a BTreeMap<String, String>,
    outputs: &'a [String],
    policy: &'a GenerationPolicy,
    memory: bool,
    available: Vec<Available>,
    next: usize,
    emitted: Option<Available>,
}
pub(crate) trait WholeReading {
    fn whole_forms(&self) -> Result<WholeForms>;
}
pub(crate) trait WholeGenerating {
    fn generate(
        &self,
        definitions: &BTreeMap<String, String>,
        signal: &Sections,
        memory: &Sections,
        operation: &Sections,
        translation: &Sections,
        policy: &GenerationPolicy,
    ) -> Result<String>;
}
trait CompositionWriting {
    fn expose(&mut self, name: &str, expression: String, ancestors: &mut Vec<String>)
    -> Result<()>;
    fn retained(&self, name: &str, expression: &str) -> Result<String>;
    fn value(&self, name: &str) -> Result<String>;
    fn compose(&self, expression: &Protos) -> Result<String>;
    fn positions(&mut self, positions: &[Protos], code: &mut String) -> Result<String>;
    fn position(&mut self, position: &Protos, code: &mut String) -> Result<String>;
}
trait TreeReading {
    fn tree(&self) -> Result<Protos>;
}
trait ShapeReading {
    fn named(&self) -> Result<&str>;
    fn body(&self) -> Option<&Protos>;
    fn children(&self) -> Result<&[Protos]>;
    fn fields(&self) -> Result<Vec<Available>>;
    fn ending_type(&self) -> Result<String>;
}
trait SourceNaming {
    fn snake(&self) -> String;
}
impl SourceNaming for str {
    fn snake(&self) -> String {
        let chars: Vec<_> = self.chars().collect();
        let mut text = String::new();
        for (index, character) in chars.iter().enumerate() {
            if character.is_uppercase()
                && index > 0
                && (chars[index - 1].is_lowercase()
                    || chars.get(index + 1).is_some_and(|c| c.is_lowercase()))
            {
                text.push('_');
            }
            text.extend(character.to_lowercase());
        }
        text
    }
}
impl TreeReading for str {
    fn tree(&self) -> Result<Protos> {
        self.protosize()
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }
}
impl ShapeReading for Protos {
    fn named(&self) -> Result<&str> {
        match self {
            Self::Bare { text, .. } => Ok(text),
            Self::Headed { head, .. } => Ok(&head.0),
            _ => bail!("expected named position"),
        }
    }
    fn body(&self) -> Option<&Protos> {
        match self {
            Self::Headed { body, .. } => Some(body),
            _ => None,
        }
    }
    fn children(&self) -> Result<&[Protos]> {
        match self {
            Self::Enclosed { children, .. } => Ok(children),
            _ => bail!("expected enclosed positions"),
        }
    }
    fn ending_type(&self) -> Result<String> {
        match self.body() {
            Some(Protos::Enclosed {
                enclosure: Enclosure::Bracketed,
                children,
                ..
            }) => {
                let ends = children
                    .iter()
                    .map(|branch| {
                        branch
                            .body()
                            .ok_or_else(|| anyhow::anyhow!("branch needs positions"))?
                            .children()?
                            .last()
                            .ok_or_else(|| anyhow::anyhow!("empty branch"))?
                            .ending_type()
                    })
                    .collect::<Result<Vec<_>>>()?;
                let first = ends
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("empty branches"))?;
                ensure!(
                    ends.iter().all(|end| end == first),
                    "branch outcomes differ"
                );
                Ok(first.clone())
            }
            _ => Ok(self.named()?.to_owned()),
        }
    }
    fn fields(&self) -> Result<Vec<Available>> {
        let Some(body) = self.body() else {
            return Ok(Vec::new());
        };
        let children = match body {
            Self::Enclosed {
                enclosure: Enclosure::Braced,
                children,
                ..
            } => children.as_slice(),
            Self::Bare { .. } | Self::Headed { .. } => std::slice::from_ref(body),
            _ => return Ok(Vec::new()),
        };
        let names = children
            .iter()
            .map(|p| p.named().map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        Ok(names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let repeated = names.iter().filter(|other| *other == name).count() > 1;
                let ordinal = names[..index].iter().filter(|other| *other == name).count();
                Available {
                    name: name.clone(),
                    expression: if repeated {
                        format!(
                            "{}_{}",
                            if ordinal == 0 { "first" } else { "second" },
                            name.snake()
                        )
                    } else {
                        name.snake()
                    },
                }
            })
            .collect())
    }
}
impl WholeReading for NexusSchema {
    fn whole_forms(&self) -> Result<WholeForms> {
        let operation = self.operation.root_sections("Operation")?;
        let mut memory = self.memory.sections("Memory", 5)?;
        let signal = self.signal.sections("Signal", 4)?;
        ensure!(
            operation.entries.len() == 4 || operation.entries.len() == 5,
            "Operation: expected five sections (or pass-three four-section body form)"
        );
        let mut attempts = Vec::new();
        let normalized = if operation.entries.len() == 5 {
            let mut inputs = Vec::new();
            let mut outcomes = Vec::new();
            for entry in &operation.entries[1] {
                let tree = entry.as_str().tree()?;
                let name = tree.named()?.to_owned();
                let positions = tree
                    .body()
                    .ok_or_else(|| anyhow::anyhow!("{name}: whole attempt requires positions"))?
                    .children()?
                    .to_vec();
                let index = signal.entries[1]
                    .iter()
                    .position(|e| e.entry_name().is_ok_and(|n| n == name));
                let outcome = if let Some(index) = index {
                    signal.entries[2][index].entry_name()?
                } else {
                    positions
                        .last()
                        .ok_or_else(|| anyhow::anyhow!("{name}: empty whole attempt"))?
                        .ending_type()?
                };
                inputs.push(name.clone());
                outcomes.push(outcome.clone());
                attempts.push(Whole {
                    name,
                    positions,
                    outcome,
                });
            }
            inputs.extend(operation.entries[2].clone());
            outcomes.extend(operation.entries[3].clone());
            format!(
                "Operation {} [ {} ] [ {} ] {}",
                operation.sections[0],
                inputs.join(" "),
                outcomes.join(" "),
                operation.sections[4]
            )
        } else {
            self.operation.clone()
        };
        let types = memory.entries[4]
            .iter()
            .map(|e| e.entry_name())
            .collect::<Result<Vec<_>>>()?;
        let mut changes = Vec::new();
        let change_outcomes = memory.entries[3].clone();
        for (index, entry) in memory.entries[2].iter_mut().enumerate() {
            let tree = entry.as_str().tree()?;
            let name = tree.named()?.to_owned();
            if types.contains(&name)
                && matches!(
                    tree.body(),
                    Some(Protos::Enclosed {
                        enclosure: Enclosure::Braced,
                        ..
                    })
                )
            {
                let positions = tree.body().unwrap().children()?.to_vec();
                let outcome = change_outcomes
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("{name}: no change outcome"))?
                    .entry_name()?;
                changes.push(Whole {
                    name: name.clone(),
                    positions,
                    outcome,
                });
                *entry = name;
            }
        }
        let memory = format!(
            "Memory {}",
            memory
                .entries
                .iter()
                .map(|entries| format!("[ {} ]", entries.join(" ")))
                .collect::<Vec<_>>()
                .join(" ")
        );
        Ok(WholeForms {
            operation: normalized,
            memory,
            attempts,
            changes,
        })
    }
}
impl WholeGenerating for WholeForms {
    fn generate(
        &self,
        definitions: &BTreeMap<String, String>,
        signal: &Sections,
        memory: &Sections,
        operation: &Sections,
        translation: &Sections,
        policy: &GenerationPolicy,
    ) -> Result<String> {
        let mut calls = BTreeMap::new();
        for (file, input, output) in [(memory, 2, 3), (operation, 1, 2), (translation, 1, 2)] {
            for (index, entry) in file.entries[input].iter().enumerate() {
                let name = entry.entry_name()?;
                let outcome = file.entries[output]
                    .get(index)
                    .or_else(|| file.entries[output].first())
                    .ok_or_else(|| anyhow::anyhow!("{name}: no outcome"))?
                    .entry_name()?;
                calls.insert(name, outcome);
            }
        }
        let outputs = signal.entries[2]
            .iter()
            .map(|e| e.entry_name())
            .collect::<Result<Vec<_>>>()?;
        let mut source = String::new();
        for whole in self.attempts.iter().chain(&self.changes) {
            let memory = self
                .changes
                .iter()
                .any(|change| std::ptr::eq(change, whole));
            ensure!(
                definitions.contains_key(&whole.name),
                "{}: declare whole attempt input type separately",
                whole.name
            );
            let mut composition = Composition {
                attempt: &whole.name,
                definitions,
                calls: &calls,
                outputs: &outputs,
                policy,
                memory,
                available: Vec::new(),
                next: 0,
                emitted: None,
            };
            composition.expose(&whole.name, "self".into(), &mut Vec::new())?;
            let mut body = String::new();
            let last = composition.positions(&whole.positions, &mut body)?;
            let value = if let Some(emitted) = &composition.emitted {
                ensure!(
                    emitted.name == whole.outcome,
                    "{}: emitted {} differs from final outcome {}",
                    whole.name,
                    emitted.name,
                    whole.outcome
                );
                composition.retained(&emitted.name, &emitted.expression)?
            } else {
                last
            };
            if memory {
                writeln!(
                    source,
                    "impl operation_scope::Changeable<NexusDomain> for {} {{type Yield={};fn change(self,memory:&mut operation_scope::Transaction<NexusDomain>)->Result<Self::Yield,Refusal>{{use operation_scope::TransactionRunning;{body}Ok({value})}}}}",
                    whole.name, whole.outcome
                )?;
            } else {
                writeln!(
                    source,
                    "impl operation_scope::Operable<NexusDomain> for {} {{type Outcome=Result<{},Refusal>;async fn operate(self,scope:&operation_scope::Scope<NexusDomain>)->Self::Outcome{{use operation_scope::Runnable;{body}Ok({value})}}}}",
                    whole.name, whole.outcome
                )?;
            }
        }
        Ok(source)
    }
}
impl CompositionWriting for Composition<'_> {
    fn expose(
        &mut self,
        name: &str,
        expression: String,
        ancestors: &mut Vec<String>,
    ) -> Result<()> {
        self.available.push(Available {
            name: name.into(),
            expression: expression.clone(),
        });
        if ancestors.iter().any(|ancestor| ancestor == name) {
            return Ok(());
        }
        let Some(declaration) = self.definitions.get(name) else {
            return Ok(());
        };
        let tree = declaration.as_str().tree()?;
        if matches!(
            tree.body(),
            Some(Protos::Enclosed {
                enclosure: Enclosure::Braced,
                ..
            })
        ) {
            ancestors.push(name.into());
            for field in tree.fields()? {
                self.expose(
                    &field.name,
                    format!("{expression}.r#{}", field.expression),
                    ancestors,
                )?;
            }
            ancestors.pop();
        }
        Ok(())
    }
    fn retained(&self, name: &str, expression: &str) -> Result<String> {
        // Ethos Zero derives Copy for fixed-size scalars and their newtype
        // chains; structs, enums and unresolved references retain Clone.
        let mut underlying = name.to_owned();
        for _ in 0..=self.definitions.len() {
            if matches!(underlying.as_str(), "Integer" | "Boolean" | "Decimal") {
                return Ok(expression.to_owned());
            }
            let Some(declaration) = self.definitions.get(&underlying) else {
                break;
            };
            let tree = declaration.as_str().tree()?;
            let Some(Protos::Bare { text, .. }) = tree.body() else {
                break;
            };
            underlying = text.clone();
        }
        Ok(format!("{expression}.clone()"))
    }
    fn value(&self, name: &str) -> Result<String> {
        let values = self
            .available
            .iter()
            .filter(|value| value.name == name)
            .collect::<Vec<_>>();
        ensure!(
            values.len() == 1,
            "{}: type {name} held {} times; expected exactly one",
            self.attempt,
            values.len()
        );
        self.retained(name, &values[0].expression)
    }
    fn compose(&self, expression: &Protos) -> Result<String> {
        let name = expression.named()?;
        let definition = self
            .definitions
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("{}: unknown type {name}", self.attempt))?;
        let shape = definition.as_str().tree()?;
        if let Some(body) = expression.body() {
            if matches!(
                shape.body(),
                Some(Protos::Enclosed {
                    enclosure: Enclosure::Bracketed,
                    ..
                })
            ) {
                let variant = body.named()?;
                let declaration = shape
                    .body()
                    .unwrap()
                    .children()?
                    .iter()
                    .find(|v| v.named().is_ok_and(|n| n == variant))
                    .ok_or_else(|| {
                        anyhow::anyhow!("{}: {name} has no variant {variant}", self.attempt)
                    })?;
                return if let Some(payload) = declaration.body() {
                    Ok(format!(
                        "{name}::{variant}({})",
                        self.value(payload.named()?)?
                    ))
                } else if self.definitions.contains_key(variant) {
                    Ok(format!("{name}::{variant}({})", self.value(variant)?))
                } else {
                    Ok(format!("{name}::{variant}"))
                };
            }
            if matches!(shape.body(), Some(Protos::Bare { .. })) {
                return Ok(format!("{name}({}.0)", self.value(body.named()?)?));
            }
            let supplied = body.children()?;
            let fields = shape.fields()?;
            ensure!(
                fields.len() == supplied.len(),
                "{}: {name} variation has wrong field count",
                self.attempt
            );
            let mut values = Vec::new();
            for (field, value) in fields.iter().zip(supplied) {
                let rendered = if value.body().is_some() {
                    self.compose(value)?
                } else {
                    self.value(value.named()?)?
                };
                values.push(format!("r#{}:{rendered}", field.expression));
            }
            return Ok(format!("{name}{{{}}}", values.join(",")));
        }
        if self.available.iter().any(|value| value.name == name) {
            return self.value(name);
        }
        if matches!(
            shape.body(),
            Some(Protos::Enclosed {
                enclosure: Enclosure::Braced,
                ..
            })
        ) {
            let fields = shape
                .fields()?
                .iter()
                .map(|field| {
                    Ok(format!(
                        "r#{}:{}",
                        field.expression,
                        self.value(&field.name)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok(format!("{name}{{{}}}", fields.join(",")));
        }
        bail!(
            "{}: type {name} held 0 times; expected exactly one",
            self.attempt
        )
    }
    fn positions(&mut self, positions: &[Protos], code: &mut String) -> Result<String> {
        let mut last = None;
        for position in positions {
            last = Some(self.position(position, code)?);
        }
        last.ok_or_else(|| anyhow::anyhow!("{}: empty composition", self.attempt))
    }
    fn position(&mut self, position: &Protos, code: &mut String) -> Result<String> {
        let name = position.named()?;
        let branches = match position.body() {
            Some(Protos::Enclosed {
                enclosure: Enclosure::Bracketed,
                children,
                ..
            }) => Some(children),
            _ => None,
        };
        let call = if branches.is_some() {
            name.to_owned().tree()?
        } else {
            position.clone()
        };
        let input = self.compose(&call)?;
        let local = format!("_position_{}", self.next);
        self.next += 1;
        let callable = self.calls.contains_key(name);
        let outcome = self
            .calls
            .get(name)
            .cloned()
            .or_else(|| branches.map(|_| name.to_owned()));
        if let Some(outcome) = &outcome {
            let held = self.policy.held_changes.iter().any(|n| n == name)
                || self.policy.held_attempts.iter().any(|n| n == name);
            if !callable {
                writeln!(code, "let {local}={input};")?;
            } else if self.memory {
                ensure!(
                    !held,
                    "{}: held change cannot be nested in a transaction",
                    self.attempt
                );
                writeln!(code, "let {local}=memory.run({input})?;")?;
            } else if self.policy.held_changes.iter().any(|n| n == name) {
                writeln!(
                    code,
                    "let {local}=operation_scope::Holding::hold(scope,{input}).await?;"
                )?;
            } else {
                writeln!(code, "let {local}=scope.run({input}).await?;")?;
            }
            let value = if held {
                format!("{local}.value")
            } else {
                local.clone()
            };
            let definition = self.definitions[outcome].as_str().tree()?;
            if let Some(Protos::Enclosed {
                enclosure: Enclosure::Bracketed,
                children: variants,
                ..
            }) = definition.body()
            {
                if let Some(branches) = branches {
                    let mut seen = std::collections::BTreeSet::new();
                    for branch in branches {
                        let branch_name = branch.named()?;
                        ensure!(
                            seen.insert(branch_name),
                            "{}: repeated branch {branch_name}",
                            self.attempt
                        );
                        ensure!(
                            variants.iter().any(|variant| variant
                                .named()
                                .is_ok_and(|name| name == branch_name)),
                            "{}: {outcome} has no branch {branch_name}",
                            self.attempt
                        );
                    }
                }
                let result = format!("_branch_{}", self.next);
                self.next += 1;
                writeln!(code, "let {result}=match {value} {{")?;
                let previous_emitted = self.emitted.take();
                let mut branch_output: Option<String> = None;
                for (index, variant) in variants.iter().enumerate() {
                    let variant_name = variant.named()?;
                    let payload = variant
                        .body()
                        .map(|body| body.named().map(str::to_owned))
                        .transpose()?
                        .or_else(|| {
                            self.definitions
                                .contains_key(variant_name)
                                .then(|| variant_name.to_owned())
                        });
                    let pattern = if payload.is_some() {
                        format!("{outcome}::{variant_name}(branch_value)")
                    } else {
                        format!("{outcome}::{variant_name}")
                    };
                    let selected = branches.and_then(|branches| {
                        branches
                            .iter()
                            .find(|branch| branch.named().is_ok_and(|n| n == variant_name))
                    });
                    let old = self.available.len();
                    if let Some(payload) = &payload {
                        self.expose(payload, "branch_value".into(), &mut Vec::new())?;
                    }
                    if let Some(branch) = selected {
                        let mut branch_code = String::new();
                        let value = self.positions(
                            branch
                                .body()
                                .ok_or_else(|| {
                                    anyhow::anyhow!("{variant_name}: branch requires positions")
                                })?
                                .children()?,
                            &mut branch_code,
                        )?;
                        let value = if let Some(emitted) = self.emitted.take() {
                            if let Some(previous) = &branch_output {
                                ensure!(
                                    previous == &emitted.name,
                                    "{}: branch outputs differ",
                                    self.attempt
                                );
                            }
                            let value = self.retained(&emitted.name, &emitted.expression)?;
                            branch_output = Some(emitted.name);
                            value
                        } else {
                            value
                        };
                        writeln!(code, "{pattern}=>{{{branch_code}{value}}},")?;
                    } else if branches.is_none() && index == 0 {
                        writeln!(
                            code,
                            "{pattern}=>{{{}}},",
                            if payload.is_some() {
                                "branch_value"
                            } else {
                                "()"
                            }
                        )?;
                    } else {
                        let refused = self
                            .definitions
                            .get("Refused")
                            .ok_or_else(|| anyhow::anyhow!("missing Refused"))?
                            .as_str()
                            .tree()?;
                        ensure!(
                            refused
                                .body()
                                .unwrap()
                                .children()?
                                .iter()
                                .any(|v| v.named().is_ok_and(|n| n == variant_name)),
                            "{}: unhandled {outcome}::{variant_name} has no same-name Refused variant",
                            self.attempt
                        );
                        writeln!(
                            code,
                            "{pattern}=>return Err(Refused::{variant_name}{}.into()),",
                            if payload.is_some() {
                                "(branch_value)"
                            } else {
                                ""
                            }
                        )?;
                    }
                    self.available.truncate(old);
                }
                writeln!(code, "}};")?;
                self.emitted = if let Some(name) = branch_output {
                    Some(Available {
                        name,
                        expression: result.clone(),
                    })
                } else {
                    previous_emitted
                };
                // Explicit branches yield their final composed value. Unbranched
                // outcomes expose the first variant's payload for later positions.
                if branches.is_none()
                    && let Some(first) = variants.first()
                    && let Some(payload) = first.body()
                {
                    self.expose(payload.named()?, result.clone(), &mut Vec::new())?;
                }
                return Ok(result);
            }
            self.expose(outcome, value.clone(), &mut Vec::new())?;
            Ok(value)
        } else {
            writeln!(code, "let {local}={input};")?;
            self.expose(name, local.clone(), &mut Vec::new())?;
            if self.outputs.iter().any(|output| output == name) && !self.memory {
                self.emitted = Some(Available {
                    name: name.to_owned(),
                    expression: local.clone(),
                });
                let retained = self.retained(name, &local)?;
                writeln!(
                    code,
                    "operation_scope::Emitting::emit(scope,operation_scope::TraceOutcome::outcome(&{local}),operation_scope::Wire::encode(&Output::{name}({retained}))?).await?;"
                )?;
            }
            Ok(local)
        }
    }
}
