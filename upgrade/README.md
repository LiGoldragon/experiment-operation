# Derived memory upgrades, pass one

`experiment-upgrade` is a standalone trial of an edit as the source of a Memory
schema change and its stored-record conversion. It uses the existing Ethos and
datom readers, and redb with checked rkyv archives.

## Tool

The executable takes one datom:

```sh
experiment-upgrade 'Initialize.{ /tmp/ledger-schema /absolute/path/memory.ethos }'
experiment-upgrade 'Edit.{ /tmp/ledger-schema AddField.{ Account { Limit Integer } Integer.100 } }'
experiment-upgrade 'Edit.{ /tmp/ledger-schema RenameRecord.{ Account LedgerAccount } }'
experiment-upgrade 'Generate.{ /absolute/path/manifest.datom /tmp/generated.rs }'
```

`current` is an atomically replaced symlink to an immutable generation containing
`memory.ethos`, `manifest.datom` and `generated.rs`. Resolve `current` once before
reading several files. Historical generations remain available. The edit tool
checks the current files against the manifest before applying the next edit.
A directory lock serializes edits. A process killed while holding that lock
leaves it for explicit recovery; this trial supplies no lock recovery service.

The manifest retains the original Memory text and ordered, numbered edits.
Generated Rust contains a module for every historical schema, typed conversion
implementations between adjacent versions, and an `UpgradePlan` that runs a
chosen prefix of the compiled history. `examples/manifest.datom` contains all
seven edit variants, with versions 0 through 7.

## Store

`Store::open` opens the redb owner; `initialize` stamps a new, empty store at 0.
`Store::upgrade` validates the target and source, runs every required conversion
inside one redb write transaction, then writes the target stamp and commits.
A failed decode, conversion, identity check or observer aborts the transaction.
Each record type carries its schema version. `insert` and `records` check that
version against the stamp inside the same transaction as the record access.
A renamed record moves its table and removes the old table in the transaction.

The integration tests compile the actual generated historical types. They prove
0-to-2 commit/reopen, rollback/reopen after each of four writes spanning two
steps, retry after rollback, native archive rejection, and the remaining edit
kinds through version 7. The generated fixture is checked byte-for-byte against
the manifest product. A failed write transaction preserves logical records and
the stamp; a redb file's physical bytes may change as pages are allocated.

## Boundaries of this trial

The accepted Memory subset is self-contained records with an inline String
identity first, inline String/Integer/Boolean fields, and references to unit
enums declared in the same file. Imported scalar aliases, recursive records,
collections and payload-bearing enums remain outside this pass. Identity edits
are refused. Each published step contains exactly one edit.

`Reshape` supplies replacement fields and a Rust expression using `old`; the
compiler checks it when the generated source is compiled. The publication tool
does not itself invoke a compiler. Generated record fields use primitive Rust
scalars rather than emitting the separate inline scalar newtypes of Ethos Zero.
This keeps the archive experiment independent of the production generator's
memory trait implementation. The generated records bear rkyv traits only.

This crate is an upgrade library and offline tool, not a running Nexus, and its
integration tests are colocated for this experiment. It does not yet join the
ledger's Scope, run or tracing path. Store opening and upgrading are explicit
calls so callers control when the migration transaction runs.

## Pass-three Nexus generation

`NexusSchema::generate_nexus_with(&GenerationPolicy)` adapts four complete
Ethos files into one checked namespace and emits types, binary codecs, identity,
trace, dispatch, startup, fingerprints, and optional text entry points. It lives
beside Ethos Zero: that crate still checks and emits every authored declaration.
`GeneratedSource::rust_source` joins its outputs for a consumer's build script.
The explicit policy selects resumable records, fallible attempts, subscriptions,
and an optional typed `memory_history::MemoryHistory`. These selections remain
outside Ethos because the trial roots have no associations surface.

`memory-manifest init <bundle> <memory-file>` initializes the new typed history;
`memory-manifest edit <bundle> '<edits-datom>'` appends one atomic step. It accepts
the five-section Memory and the two-section storage projection. Immutable
Memory/manifest generations are synced before one `current` symlink changes;
a reader resolves that symlink once. A writer killed while holding `writer-lock`
requires explicit recovery. This bootstrap tool still uses ordinary arguments.

Historical types and conversions come from the manifest, not handwritten
copies. The generated Upgrade runs in the store owner's first transaction;
its records and the final stamp commit together. It checks identities, validates
archives, refuses unknown stamps and refuses a manifest that revisits a schema.
Rename edits, imported/recursive conversions, inline historical field types,
and more than four equal field types remain outside the checked subset.
Default and Reshape bodies are Rust expressions checked when generated Rust
compiles; publishing the manifest does not compile those expressions.

The adapter presently requires a self-contained combined namespace, bare Signal
variants paired positionally, and at least two inputs and outputs. Generated
`Output::Runtime` adds the common runtime refusal. Ordinary binary dependencies
contain no codec: Client, MetaClient and Debugger are gated by `datom` and take
the binary launch-configuration path before their datom (debugger: path alone).

The trial's additional ruling from Psyche core Opus is implemented: Signal's
fingerprint hashes the canonical expanded contract, including all reachable
payload definitions across the four authored files. Storage-only changes do
not change that fingerprint. The generated standard Runtime payload still
uses library Rust Failure, outside the authored Ethos closure; changing that
implicit vocabulary is not yet included in the contract fingerprint.
