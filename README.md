# Operation experiment

The current trial is [pass four](pass-three/README.md), retained in the original
`pass-three/` consumer paths: generated whole attempts and changes, attempt-owned
Memory records, open lock exchanges, and process-death recovery. Earlier passes
remain historical evidence in their own crates.

## Pass 1

Public experimental code for the operations-as-objects design. This repository is a trial, not a production Nexus release.

`crates/scope-ledger` supplies a single generic `Scope.run` and identical ledger operation bodies with two redb store owners:

- `tasks`: one Tokio task and bounded channel.
- `actors`: one Kameo actor and its mailbox.

`upgrade` is the separate Memory edit and upgrade trial.

## Build and run

Build-base supplies the compiler, package set and crate derivations. The root flake includes both crates and checks the tracing feature of scope-ledger. Generate Cargo.nix after manifest changes using the base toolchain and crate2nix on the builder. The full `checks` surface includes base registry checks; newly explored dependencies can fail those checks independently of member tests.

`scope-ledger tasks|actors STORE ITERATIONS` measures sequential root-task Recall operations over an existing redb account. `ITERATIONS=crash` stops the process after a committed withdrawal, without destructors; the process-stop integration scenario verifies recovery. The trace feature includes full input and output records; a plain build has no trace fields or checkpoint formatting calls. Store capacity is a parameter of `Scope::open`.

## Deliberate experimental limits

The binary is a local operation driver, without ordinary/meta sockets, translators or peer authentication. Ask, Translation, subscriptions and world-event routing have not been implemented. There are no connections in either backend to turn into actors. Generic records implement Memorizable's identity and checked rkyv codec by hand. The Ethos files preserve the design vocabulary; they do not generate the Rust implementation. Atomic accepts store changes, not arbitrary operations or world effects. Ledger balances are signed machine integers and reject negative changes/overflow. A transfer's two commits follow the design and are deliberately not one transaction: cancellation, process termination or a missing destination can leave a committed debit. Concurrent read-modify-write operations are not serializable as complete roots.

Trace textualization is a trial codec, not datom-codec derivation. Names and error text containing string delimiters are not escaped. Atomic traces expose exact stored archive bytes; the trace is not a stable cross-version logical schema. Root traces have the empty path; child paths are allocated on entry, and independent concurrent roots are not assigned separate trace identifiers. Pending cancellations leave an Entered without a Yielded. There is no durable operation journal or replay.
