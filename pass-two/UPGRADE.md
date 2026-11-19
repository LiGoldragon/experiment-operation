# The upgrade on the ledger's owner thread

The same store owner that runs ledger changes runs the generated upgrade before
it reports readiness. `Scope::open` prepares it by reading the schema stamp,
enters it through `Transaction::run`, and commits its records and stamp together.
Only then does Resume run. An unknown stamp refuses before archived records are
decoded, naming the stored and compiled fingerprints.

## Authored and generated sources

- `schema/edit.ethos` declares Edit, Step, Manifest and their vocabulary.
  `schema/edit.rs` is Ethos Zero's output, proved exact by `build.rs`.
- `schema/tool.rs` parses Memory through Ethos Zero, hashes its canonical print
  with SHA-256, applies checked edits, and derives historical Rust and conversions.
- `schema/original.ethos` and `schema/edits.datom` are the trial input.
- `schema/manifest.datom`, `schema/memory.ethos` and `schema/history.rs` are the
  published generation. The upgrade tests prove the history is exactly derived.
- The chain also emits the Upgrade type and its Changeable implementation.
  `src/upgrade.rs` owns stamp checking and atomic table migration;
  `src/memory.rs` owns synchronous archive access.

The tool holds a single-writer directory lock, builds a complete immutable
generation, syncs its files and directory, then atomically switches `current`.
A consumer resolves `current` once. A stale writer lock after a crash requires
explicit recovery; generations left before publication are harmless.

```sh
manifest init /tmp/trial-bundle pass-two/schema/original.ethos
manifest edit /tmp/trial-bundle "$(cat pass-two/schema/edits.datom)"
```

A step contains several edits and names the SHA-256 fingerprint it reaches.
The manifest also retains original Memory text so earlier types can be derived
without another historical-source lookup. This adds a source field to the sketch.

## Widened trial and boundaries

The fixture upgrades an Envelope with an integer identity, vector, option,
nested record and a data-carrying enum. Both edits occur in one step. Account
and Reservation remain in the same database and migration chain as that Envelope.
Migration checks keys against decoded identities and preserves identities.

The compiler supports named, nonrecursive types and primitive first-field
identities. It derives vector, option, named-record and typed-enum conversions.
AddField, RemoveField, unit AddVariant, RemoveVariant to a surviving unit variant,
and explicit Reshape are implemented. RenameField and RenameRecord are declared
but explicitly refused. Inline field and enum payload declarations require
named types. Added-field/reshape fragments are parsed separately and must be
self-contained; adding references into the existing schema needs contextual parsing.
Imported/recursive conversions, arbitrary identity changes and more
than four repeated field types are outside this bounded compiler. Default and
Reshape bodies are Rust expressions checked when the emitted Rust is compiled.

Ethos Zero at the pinned revision reads only Memory's imports and records, not
the design's additional change/outcome sections. Changes and the common runner
are therefore handwritten Rust. Fingerprints cover the complete supported Memory
file, including order; no claim is made for unsupported sections.

The feature-empty runtime library excludes the manifest tool, Edit text codecs
and optional runtime text dependencies (the build script still uses Ethos). Default/datom enables the drivers and tool; trace
also enables datom. There is no feature-empty executable in this trial crate.
The manifest executable uses ordinary arguments rather than a Nexus datom surface.
