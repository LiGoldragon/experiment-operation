//! Pass-three generation beside Ethos Zero. The adapter extends its file roots;
//! Ethos Zero remains the only type reader, checker and Rust type emitter.
use anyhow::{Result, bail, ensure};
use ethos_zero::{
    Actualizing, Canonicalizable, File, Generating, Position, Potential, Printable, TypeDeclaration,
};
use protos::{Enclosure, Protos, Protosizable, Textualizable};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::fmt::Write;

/// The four authored files of one Nexus, resolved as one declaration namespace.
pub struct NexusSchema {
    pub signal: String,
    pub memory: String,
    pub operation: String,
    pub translation: String,
}
/// Build-time outputs. No text reader is linked into the resulting runtime.
pub struct GeneratedNexus {
    pub types: String,
    pub bindings: String,
    pub trace: String,
    pub fingerprints: String,
}
pub trait GeneratedSource {
    fn rust_source(&self) -> String;
}
impl GeneratedSource for GeneratedNexus {
    fn rust_source(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.types, self.trace, self.fingerprints, self.bindings
        )
    }
}
#[derive(Default)]
pub struct GenerationPolicy {
    pub resumables: Vec<String>,
    pub fallible_attempts: Vec<String>,
    pub subscriptions: Vec<SubscriptionBinding>,
    pub memory_history: Option<crate::memory_history::MemoryHistory>,
}
pub struct SubscriptionBinding {
    pub input: String,
    pub record: String,
    pub output: String,
}
pub trait GeneratingNexus {
    fn generate_nexus(&self) -> Result<GeneratedNexus> {
        self.generate_nexus_with(&GenerationPolicy::default())
    }
    fn generate_nexus_with(&self, policy: &GenerationPolicy) -> Result<GeneratedNexus>;
}
pub(crate) struct Sections {
    pub(crate) sections: Vec<String>,
    pub(crate) entries: Vec<Vec<String>>,
}
pub(crate) trait ReadingSections {
    fn root_sections(&self, root: &str) -> Result<Sections>;
    fn sections(&self, root: &str, count: usize) -> Result<Sections> {
        let sections = self.root_sections(root)?;
        ensure!(
            sections.sections.len() == count,
            "{root}: expected {count} sections, found {}",
            sections.sections.len()
        );
        Ok(sections)
    }
}
impl ReadingSections for str {
    fn root_sections(&self, root: &str) -> Result<Sections> {
        let canonical = self
            .to_owned()
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let tree = canonical
            .text
            .protosize()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let Protos::Headed { head, body, .. } = &tree else {
            bail!("expected {root} root")
        };
        ensure!(head.0 == root, "expected {root}, found {}", head.0);
        let Protos::Enclosed {
            enclosure: Enclosure::Braced,
            children,
            ..
        } = &**body
        else {
            bail!("expected root sections")
        };
        let mut sections = Vec::new();
        let mut entries = Vec::new();
        for child in children {
            let Protos::Enclosed {
                enclosure: Enclosure::Bracketed,
                children: items,
                ..
            } = child
            else {
                bail!("{root}: each section must be a bracket")
            };
            sections.push(child.textualize());
            entries.push(items.iter().map(Textualizable::textualize).collect());
        }
        Ok(Sections { sections, entries })
    }
}
pub(crate) trait NamedEntry {
    fn entry_name(&self) -> Result<String>;
    fn object_declaration(&self) -> Result<String>;
}
impl NamedEntry for str {
    fn object_declaration(&self) -> Result<String> {
        match &self.protosize().map_err(|e| anyhow::anyhow!("{e:?}"))? {
            Protos::Bare { text, .. } => Ok(format!("{text}.{{}}")),
            Protos::Headed { head, body, .. }
                if matches!(**body, Protos::Bare { .. } | Protos::Headed { .. }) =>
            {
                Ok(format!("{}.{{ {} }}", head.0, body.textualize()))
            }
            _ => Ok(self.to_owned()),
        }
    }
    fn entry_name(&self) -> Result<String> {
        match &self.protosize().map_err(|e| anyhow::anyhow!("{e:?}"))? {
            Protos::Bare { text, .. } => Ok(text.clone()),
            Protos::Headed { head, .. } => Ok(head.0.clone()),
            _ => bail!("expected a named declaration: {self}"),
        }
    }
}
impl GeneratingNexus for NexusSchema {
    fn generate_nexus_with(&self, policy: &GenerationPolicy) -> Result<GeneratedNexus> {
        let signal = self.signal.sections("Signal", 4)?;
        let memory = self.memory.sections("Memory", 5)?;
        let operation = self.operation.sections("Operation", 4)?;
        let translation = self.translation.sections("Translation", 4)?;
        // Imports cannot be silently discarded: external namespaces need an
        // Ethos repository resolver, absent from this bounded experiment.
        for file in [&signal, &memory, &operation, &translation] {
            ensure!(
                file.entries[0].is_empty(),
                "pass-three adapter requires a self-contained combined namespace; imports are unsupported"
            );
        }
        let mut declarations = BTreeMap::new();
        for (file, index) in [
            (&memory, 1),
            (&memory, 4),
            (&signal, 3),
            (&operation, 3),
            (&translation, 3),
        ] {
            for entry in &file.entries[index] {
                let name = entry.entry_name()?;
                if let Some(existing) = declarations.insert(name.clone(), entry.clone()) {
                    ensure!(existing == *entry, "conflicting declaration {name}");
                }
            }
        }
        // Operations are named objects, rather than variants hidden inside a
        // root-generated enum. A unit operation is an empty data-bearing type.
        for (file, index) in [
            (&memory, 2),
            (&memory, 3),
            (&operation, 1),
            (&operation, 2),
            (&translation, 1),
            (&translation, 2),
        ] {
            for entry in &file.entries[index] {
                let name = entry.entry_name()?;
                match declarations.entry(name.clone()) {
                    Entry::Vacant(slot) => {
                        slot.insert(entry.object_declaration()?);
                    }
                    Entry::Occupied(slot) if entry != &name => ensure!(
                        slot.get() == &entry.object_declaration()? || slot.get() == entry,
                        "conflicting declaration {name}"
                    ),
                    Entry::Occupied(_) => {}
                }
            }
        }
        for reserved in [
            "Input",
            "Output",
            "WorldInput",
            "Object",
            "Outcome",
            "Event",
            "TraceRecord",
            "Refusal",
            "Upgrade",
            "UpgradeYield",
            "Nexus",
            "NexusDomain",
            "Client",
            "MetaClient",
            "Debugger",
            "TextRunning",
        ] {
            ensure!(
                !declarations.contains_key(reserved),
                "{reserved} is a generated infrastructure name"
            );
        }
        let mut all = declarations.values().cloned().collect::<Vec<_>>();
        all.push(format!("Input.[ {} ]", signal.entries[1].join(" ")));
        all.push(format!("Output.[ {} ]", signal.entries[2].join(" ")));
        if !translation.entries[1].is_empty() {
            all.push(format!(
                "WorldInput.[ {} ]",
                translation.entries[1]
                    .iter()
                    .map(|e| e.entry_name())
                    .collect::<Result<Vec<_>>>()?
                    .join(" ")
            ));
        }
        let library = format!("Library [] [ {} ] [] []", all.join("\n"));
        let parsed = Potential::<File>::from(library)
            .actualize()
            .map_err(|e| anyhow::anyhow!("combined Nexus declarations: {e:?}"))?;
        let mut types = parsed
            .generate()
            .map_err(|e| anyhow::anyhow!("Nexus Rust: {e:?}"))?;
        types = types.replace("#![", "#[").replace(", Hash", "");
        let output_start = types
            .find("pub enum Output")
            .expect("Output enum generated");
        let output_end =
            output_start + types[output_start..].find('}').expect("Output enum closes");
        types.insert_str(output_end, " Runtime(operation_scope::Failure),\n");
        let storage_text = format!(
            "Memory {} [ {} {} ]",
            memory.sections[0],
            memory.entries[1].join(" "),
            memory.entries[4].join(" ")
        );
        let storage = Potential::<File>::from(storage_text)
            .actualize()
            .map_err(|e| anyhow::anyhow!("Memory storage declarations: {e:?}"))?;
        let File::Library(library) = &parsed else {
            unreachable!()
        };
        let definitions = library
            .types
            .iter()
            .map(|declaration| (declaration.declaration_name().to_owned(), declaration))
            .collect::<BTreeMap<_, _>>();
        let TypeDeclaration::Enum(_, inputs) = definitions["Input"] else {
            unreachable!()
        };
        let TypeDeclaration::Enum(_, outputs) = definitions["Output"] else {
            unreachable!()
        };
        let mut reached = BTreeSet::new();
        definitions["Input"].reach(&definitions, &mut reached)?;
        definitions["Output"].reach(&definitions, &mut reached)?;
        // Keep every declaration the Signal itself writes, in its own order,
        // then append its reachable definitions canonically by name.
        let mut expanded_types = Vec::new();
        for entry in &signal.entries[3] {
            let name = entry.entry_name()?;
            expanded_types.push(definitions[&name].clone());
            reached.remove(&name);
        }
        for name in reached {
            expanded_types.push(definitions[&name].clone());
        }
        let expanded = File::Signal(ethos_zero::Signal {
            imports: Vec::new(),
            queries: inputs.clone(),
            responses: outputs.clone(),
            types: expanded_types,
        });
        let canonical = expanded.print();
        let memory_digest = Sha256::digest(storage.print().as_bytes());
        let signal_digest = Sha256::digest(canonical.as_bytes());
        let fingerprints = format!(
            "pub const MEMORY_FINGERPRINT: [u8;32] = {:?};\npub const SIGNAL_FINGERPRINT: [u8;32] = {:?};\n",
            memory_digest.as_slice(),
            signal_digest.as_slice()
        );
        let mut objects = Vec::new();
        let mut outcomes = Vec::new();
        for (file, index) in [
            (&memory, 2),
            (&operation, 1),
            (&translation, 1),
            (&memory, 1),
        ] {
            for entry in &file.entries[index] {
                let name = entry.entry_name()?;
                if !objects.contains(&name) {
                    objects.push(name);
                }
            }
        }
        for (file, index) in [
            (&memory, 3),
            (&operation, 2),
            (&translation, 2),
            (&memory, 1),
        ] {
            for entry in &file.entries[index] {
                let name = entry.entry_name()?;
                if !outcomes.contains(&name) {
                    outcomes.push(name);
                }
            }
        }
        for entry in &memory.entries[1] {
            let name = entry.entry_name()?;
            if !outcomes.contains(&name) {
                outcomes.push(name);
            }
        }
        ensure!(
            declarations.contains_key("Refused"),
            "declare domain Refused among Memory outcomes"
        );
        let mut bindings = String::new();
        let File::Library(library) = &parsed else {
            unreachable!()
        };
        for declaration in &library.types {
            bindings.push_str(&declaration.wire_binding()?);
        }
        for record in &memory.entries[1] {
            let name = record.entry_name()?;
            let declaration = library
                .types
                .iter()
                .find(|d| d.declaration_name() == name)
                .expect("record was parsed");
            let TypeDeclaration::Struct(_, fields) = declaration else {
                bail!("record {name}: identity requires a struct with a first field")
            };
            let Some(first) = fields.first() else {
                bail!("record {name}: identity requires a first field")
            };
            let mut field = first.field_name()?;
            if fields
                .iter()
                .skip(1)
                .any(|position| position.field_name().is_ok_and(|name| name == field))
            {
                field = format!("first_{field}");
            }
            let field = format!("r#{field}");
            writeln!(
                bindings,
                "impl operation_scope::Memorizable for {name} {{ const TABLE: &'static str = {name:?}; fn identity(&self)->Vec<u8> {{ rkyv::to_bytes::<rkyv::rancor::Error>(&self.{field}).expect(\"serializable identity\").to_vec() }} }}"
            )?;
        }
        bindings.push_str("#[derive(Clone)] pub struct NexusDomain;\nimpl operation_scope::Domain for NexusDomain { type Object=Object; type Outcome=Outcome; type Refusal=Refusal; fn upgrade(memory:&mut operation_scope::Transaction<Self>,stored:[u8;32])->Result<(),operation_scope::Failure> {operation_scope::TransactionRunning::run(memory,Upgrade{from:stored.into_iter().map(i64::from).collect()}).map(|_|()).map_err(|error|match error {Refusal::Runtime(error)=>error,Refusal::Domain(_)=>operation_scope::Failure::Wire})} const MEMORY_FINGERPRINT:[u8;32]=MEMORY_FINGERPRINT; const SIGNAL_FINGERPRINT:[u8;32]=SIGNAL_FINGERPRINT; }\n");
        for name in &objects {
            writeln!(
                bindings,
                "impl operation_scope::TraceObject<NexusDomain> for {name} {{ fn object(&self)->Object {{ Object::{name}(self.clone()) }} }}"
            )?;
        }
        for name in &outcomes {
            writeln!(
                bindings,
                "impl operation_scope::TraceOutcome<NexusDomain> for {name} {{ fn outcome(&self)->Outcome {{ Outcome::{name}(self.clone()) }} }}"
            )?;
        }
        for (file, index, method, capability) in [
            (&memory, 2, "change", "ChangeExecuting"),
            (&operation, 1, "attempt", "AttemptExecuting"),
        ] {
            for entry in &file.entries[index] {
                let name = entry.entry_name()?;
                let body_trait = if index == 2 { "Changeable" } else { "Operable" };
                let associated = if index == 2 { "Yield" } else { "Outcome" };
                writeln!(
                    bindings,
                    "impl operation_scope::ScopeObject<NexusDomain> for {name} {{ type Outcome = {} ; async fn execute(self,scope:&operation_scope::Scope<NexusDomain>)->Self::Outcome {{ operation_scope::{capability}::{method}(scope,self).await }} }}",
                    if index == 2 {
                        format!(
                            "Result<<Self as operation_scope::{body_trait}<NexusDomain>>::{associated}, Refusal>"
                        )
                    } else {
                        format!(
                            "<Self as operation_scope::{body_trait}<NexusDomain>>::{associated}"
                        )
                    }
                )?;
            }
        }
        bindings.push_str("#[derive(Clone)] pub struct Nexus;\nimpl operation_scope::Serving<NexusDomain> for Nexus { type Input=Input; type Output=Output; async fn dispatch(&self,input:Input,scope:&operation_scope::Scope<NexusDomain>)->Result<operation_scope::Delivery<Output>,operation_scope::Failure> { use operation_scope::Runnable; Ok(match input {\n");
        for (index, input) in signal.entries[1].iter().enumerate() {
            let name = input.entry_name()?;
            ensure!(
                *input == name,
                "Signal inputs must name their operation type bare: {input}"
            );
            let output = signal.entries[2]
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("Signal input {name} has no paired output"))?;
            let output_name = output.entry_name()?;
            ensure!(
                *output == output_name,
                "Signal outputs must name their yield type bare: {output}"
            );
            if let Some(subscription) = policy
                .subscriptions
                .iter()
                .find(|subscription| subscription.input == name)
            {
                ensure!(
                    subscription.output == output_name,
                    "subscription {} output differs from Signal",
                    name
                );
                let record = &subscription.record;
                let field = record.snake_name();
                writeln!(
                    bindings,
                    "Input::{name}(object) => match scope.run(object).await {{ Ok(subscription)=>{{use operation_scope::SubscriptionStreaming;subscription.delivery(|bytes| {{use operation_scope::Wire;Ok(Output::{output_name}({output_name} {{ {field}: {record}::decode(bytes)? }}))}})}}, Err(Refusal::Domain(value))=>operation_scope::Delivery::Reply(Output::Refused(value)),Err(Refusal::Runtime(error))=>operation_scope::Delivery::Reply(Output::Runtime(error)) }},"
                )?;
            } else if memory.entries[2]
                .iter()
                .any(|x| x.entry_name().is_ok_and(|n| n == name))
            {
                writeln!(
                    bindings,
                    "Input::{name}(object) => match scope.run(object).await {{Ok(value)=>operation_scope::Delivery::Reply(Output::{output_name}(value)),Err(Refusal::Domain(value))=>operation_scope::Delivery::Reply(Output::Refused(value)),Err(Refusal::Runtime(error))=>operation_scope::Delivery::Reply(Output::Runtime(error))}},"
                )?;
            } else {
                ensure!(
                    operation.entries[1]
                        .iter()
                        .any(|x| x.entry_name().is_ok_and(|n| n == name)),
                    "Signal input {name} has no change or attempt"
                );
                if policy.fallible_attempts.contains(&name) {
                    writeln!(
                        bindings,
                        "Input::{name}(object)=>match scope.run(object).await {{Ok(value)=>operation_scope::Delivery::Reply(Output::{output_name}(value)),Err(Refusal::Domain(value))=>operation_scope::Delivery::Reply(Output::Refused(value)),Err(Refusal::Runtime(error))=>operation_scope::Delivery::Reply(Output::Runtime(error))}},"
                    )?;
                } else {
                    writeln!(
                        bindings,
                        "Input::{name}(object)=>operation_scope::Delivery::Reply(Output::{output_name}(scope.run(object).await)),"
                    )?;
                }
            }
        }
        bindings.push_str("})} }\n");
        bindings.push_str("impl operation_scope::Starting<NexusDomain> for Nexus {async fn start(&self,scope:&operation_scope::Scope<NexusDomain>)->Result<(),operation_scope::Failure> { let _=scope;\n");
        if !policy.resumables.is_empty() {
            bindings.push_str("use operation_scope::Resuming;\n");
        }
        for name in &policy.resumables {
            ensure!(
                memory.entries[1]
                    .iter()
                    .any(|e| e.entry_name().is_ok_and(|n| n == *name)),
                "resumable {name} is not a Memory record"
            );
            writeln!(
                bindings,
                "scope.resume_records::<{name}>().await.map_err(|_|operation_scope::Failure::Stopped)?;"
            )?;
        }
        bindings.push_str("Ok(())}}\n");
        for name in &policy.resumables {
            writeln!(
                bindings,
                "impl operation_scope::Resumable<NexusDomain> for {name} {{}}\nimpl operation_scope::ScopeObject<NexusDomain> for {name} {{type Outcome=<Self as operation_scope::Operable<NexusDomain>>::Outcome;async fn execute(self,scope:&operation_scope::Scope<NexusDomain>)->Self::Outcome {{operation_scope::AttemptExecuting::attempt(scope,self).await}}}}"
            )?;
        }
        objects.push("Upgrade".into());
        outcomes.push("UpgradeYield".into());
        bindings.push_str("impl operation_scope::TraceObject<NexusDomain> for Upgrade {fn object(&self)->Object{Object::Upgrade(self.clone())}}\nimpl operation_scope::TraceOutcome<NexusDomain> for UpgradeYield {fn outcome(&self)->Outcome{Outcome::UpgradeYield(self.clone())}}\n");
        let derives = "#[derive(Clone,Debug,PartialEq,Eq,rkyv::Archive,rkyv::Serialize,rkyv::Deserialize)]\n#[cfg_attr(feature=\"datom\",derive(datom_codec::Datomizable,datom_codec::Composing))]\n";
        let mut trace = format!(
            "{derives}pub enum Object {{ {} }}\n{derives}pub enum Outcome {{ {}, Refusal(Refusal) }}\n{derives}pub enum Refusal {{ Runtime(operation_scope::Failure), Domain(Refused) }}\n",
            objects
                .iter()
                .map(|n| format!("{n}({n})"))
                .collect::<Vec<_>>()
                .join(","),
            outcomes
                .iter()
                .map(|n| format!("{n}({n})"))
                .collect::<Vec<_>>()
                .join(",")
        );
        write!(
            trace,
            "{derives}pub enum Event {{ Entered(Object),Yielded(Outcome),Emitted(Outcome),Ended,Abandoned }}\n{derives}pub struct TraceRecord {{ pub root:i64,pub path:Vec<i64>,pub event:Event }}\n"
        )?;
        trace.push_str("impl operation_scope::TraceEncoding<NexusDomain> for Nexus {fn encode_trace(trace:operation_scope::Trace<NexusDomain>)->Result<Vec<u8>,operation_scope::Failure> {use operation_scope::Wire;let event=match trace.event {operation_scope::Event::Entered(v)=>Event::Entered(v),operation_scope::Event::Yielded(v)=>Event::Yielded(v),operation_scope::Event::Emitted(v)=>Event::Emitted(v),operation_scope::Event::Ended=>Event::Ended,operation_scope::Event::Abandoned=>Event::Abandoned};TraceRecord{root:i64::try_from(trace.root).map_err(|_|operation_scope::Failure::Wire)?,path:trace.path.into_iter().map(i64::try_from).collect::<Result<Vec<_>,_>>().map_err(|_|operation_scope::Failure::Wire)?,event}.encode()} }\n");
        write!(
            trace,
            "{derives}pub struct Upgrade {{pub from:Vec<i64>}}\n{derives}pub struct UpgradeYield {{pub from:Vec<i64>,pub to:Vec<i64>,pub records:i64}}\n"
        )?;
        trace.push_str("impl operation_scope::Changeable<NexusDomain> for Upgrade {type Yield=UpgradeYield;fn change(self,memory:&mut operation_scope::Transaction<NexusDomain>)->Result<UpgradeYield,Refusal> {let stored:[u8;32]=self.from.iter().map(|v|u8::try_from(*v)).collect::<Result<Vec<_>,_>>().map_err(|_|operation_scope::Failure::Wire)?.try_into().map_err(|_|operation_scope::Failure::Wire)?;#[allow(unused_mut)]let mut records=0i64;if stored!=[0;32] && stored!=MEMORY_FINGERPRINT {\n");
        if let Some(history) = &policy.memory_history {
            use crate::memory_history::GeneratingHistory;
            let records = memory.entries[1]
                .iter()
                .map(|e| e.entry_name())
                .collect::<Result<Vec<_>>>()?;
            let generated = history.generate_history(&storage, &records)?;
            bindings.push_str(&generated.source);
            trace.push_str(&generated.migration);
        } else {
            trace.push_str("return Err(operation_scope::Failure::Schema{stored,expected:MEMORY_FINGERPRINT}.into());\n");
        }
        trace.push_str("}let _=memory;Ok(UpgradeYield{from:self.from,to:MEMORY_FINGERPRINT.into_iter().map(i64::from).collect(),records})}}\n");
        for name in [
            "Object",
            "Outcome",
            "Refusal",
            "Event",
            "TraceRecord",
            "Upgrade",
            "UpgradeYield",
        ] {
            trace.push_str(&name.wire_source());
        }
        trace.push_str("impl std::convert::From<operation_scope::Failure> for Refusal { fn from(e:operation_scope::Failure)->Self { Self::Runtime(e) } }\nimpl std::convert::From<Refused> for Refusal { fn from(e:Refused)->Self {Self::Domain(e)} }\nimpl std::convert::From<operation_scope::Failure> for Outcome { fn from(e:operation_scope::Failure)->Self {Self::Refusal(e.into())} }\nimpl operation_scope::TraceOutcome<NexusDomain> for Refusal { fn outcome(&self)->Outcome {Outcome::Refusal(self.clone())} }\n");
        let subscription_match = if policy.subscriptions.is_empty() {
            "false".to_owned()
        } else {
            format!(
                "matches!(&input,{})",
                policy
                    .subscriptions
                    .iter()
                    .map(|s| format!("Input::{}(_)", s.input))
                    .collect::<Vec<_>>()
                    .join("|")
            )
        };
        bindings.push_str(
            &crate::nexus_text::TEXT_CLIENTS.replace("IS_SUBSCRIPTION", &subscription_match),
        );
        Ok(GeneratedNexus {
            types,
            bindings,
            trace,
            fingerprints,
        })
    }
}
trait DeclarationBinding {
    fn declaration_name(&self) -> &str;
    fn wire_binding(&self) -> Result<String>;
}
impl DeclarationBinding for TypeDeclaration {
    fn declaration_name(&self) -> &str {
        match self {
            Self::Struct(i, _) | Self::Enum(i, _) | Self::Alias(i, _) => i.name.as_ref(),
        }
    }
    fn wire_binding(&self) -> Result<String> {
        Ok(self.declaration_name().wire_source())
    }
}
trait WireSource {
    fn wire_source(&self) -> String;
}
impl WireSource for str {
    fn wire_source(&self) -> String {
        format!(
            "impl operation_scope::Wire for {self} {{ fn encode(&self)->Result<Vec<u8>,operation_scope::Failure> {{ rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|b|b.to_vec()).map_err(|_|operation_scope::Failure::Wire) }} fn decode(bytes:&[u8])->Result<Self,operation_scope::Failure> {{ rkyv::from_bytes::<Self,rkyv::rancor::Error>(bytes).map_err(|_|operation_scope::Failure::Wire) }} }}\n"
        )
    }
}
trait FieldNaming {
    fn field_name(&self) -> Result<String>;
}
impl FieldNaming for Position {
    fn field_name(&self) -> Result<String> {
        match self {
            Self::Referenced(reference) => {
                ensure!(
                    reference.arguments.is_empty(),
                    "record identity must be a scalar or named type"
                );
                Ok(reference.name.as_ref().snake_name())
            }
            Self::Declared(declaration) => Ok(declaration.declaration_name().snake_name()),
        }
    }
}
trait SnakeNaming {
    fn snake_name(&self) -> String;
}
impl SnakeNaming for str {
    fn snake_name(&self) -> String {
        let chars: Vec<_> = self.chars().collect();
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

/// The canonical wire closure, independent of a declaration's authored file.
trait ReachingDefinitions {
    fn reach(
        &self,
        definitions: &BTreeMap<String, &TypeDeclaration>,
        reached: &mut BTreeSet<String>,
    ) -> Result<()>;
}
impl ReachingDefinitions for ethos_zero::Reference {
    fn reach(
        &self,
        definitions: &BTreeMap<String, &TypeDeclaration>,
        reached: &mut BTreeSet<String>,
    ) -> Result<()> {
        ensure!(
            self.source.is_none(),
            "external wire definitions require a repository resolver"
        );
        let name = self.name.as_ref();
        if let Some(declaration) = definitions.get(name)
            && reached.insert(name.to_owned())
        {
            declaration.reach(definitions, reached)?;
        }
        for argument in &self.arguments {
            argument.reach(definitions, reached)?;
        }
        Ok(())
    }
}
impl ReachingDefinitions for Position {
    fn reach(
        &self,
        definitions: &BTreeMap<String, &TypeDeclaration>,
        reached: &mut BTreeSet<String>,
    ) -> Result<()> {
        match self {
            Self::Referenced(reference) => reference.reach(definitions, reached),
            Self::Declared(declaration) => declaration.reach(definitions, reached),
        }
    }
}
impl ReachingDefinitions for ethos_zero::Variant {
    fn reach(
        &self,
        definitions: &BTreeMap<String, &TypeDeclaration>,
        reached: &mut BTreeSet<String>,
    ) -> Result<()> {
        match self {
            Self::Bare(name) => {
                if let Some(declaration) = definitions.get(name.as_ref())
                    && reached.insert(name.as_ref().to_owned())
                {
                    declaration.reach(definitions, reached)?;
                }
            }
            Self::Typed(_, reference) => reference.reach(definitions, reached)?,
            Self::Struct(_, fields) => {
                for field in fields {
                    field.reach(definitions, reached)?;
                }
            }
            Self::Enum(_, variants) => {
                for variant in variants {
                    variant.reach(definitions, reached)?;
                }
            }
        }
        Ok(())
    }
}
impl ReachingDefinitions for TypeDeclaration {
    fn reach(
        &self,
        definitions: &BTreeMap<String, &TypeDeclaration>,
        reached: &mut BTreeSet<String>,
    ) -> Result<()> {
        match self {
            Self::Alias(_, reference) => reference.reach(definitions, reached)?,
            Self::Struct(_, fields) => {
                for field in fields {
                    field.reach(definitions, reached)?;
                }
            }
            Self::Enum(_, variants) => {
                for variant in variants {
                    variant.reach(definitions, reached)?;
                }
            }
        }
        Ok(())
    }
}
