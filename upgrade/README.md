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
