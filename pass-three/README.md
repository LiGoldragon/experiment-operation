# Pass four: whole attempts and attempt-owned holding

This directory keeps its pass-three path so earlier consumers stay in place.
The current trial implements Attempt.1 proposals 3, 4, 5 and 7. Earlier runtime
passes remain separate evidence elsewhere in the workspace.

`operation-scope` owns execution, transactional Memory, binary connections and
trace delivery. `trial-ledger` and `trial-lock` consume `experiment-upgrade`'s
build-time generator beside Ethos Zero. Their four `ethos/` files generate
`OUT_DIR/nexus.rs`; no text reader is linked into the ordinary runtime.

## What is generated

Operation has imports, whole attempts, body-bearing attempts, their outcomes,
and types. A whole attempt's input type is declared separately in types. Memory
keeps its five sections; a change whose name also has a separately declared
input shape is interpreted as a composition. The generator reports missing or
ambiguous typed inputs before returning generated Rust.

Pay, Reservation recovery, Acknowledge and Lock are whole attempts. Transfer
is a whole change over Withdraw and Deposit; both run in the same transaction.
Branches follow typed outcomes. Domain refusals propagate through the root;
an unmatched outcome needs a same-name Refused variant. Explicit call variations
convert the distinct From/To scalar wrappers to Name for Transfer.

Generation policy still supplies resumable records, fallible and held call
associations and subscription bindings. Leaf bodies still include their Rust
impl headers and associated yields. This remains a bounded adapter rather than
an implementation of the full proposed Ethos language.

## Holding and outputs

`Holding::hold` establishes an owner guard before submitting a leaf change.
`Transaction::remember_held` stores the record and its owner-index entry in the
same commit. Dropping the guard queues removal on that same store owner. A
cancelled acquisition therefore also cleans up if its commit already happened.
Opening Memory removes every owner-indexed record before startup completes.
The owner index, rather than a field of each domain record, holds its runtime
owner number. Historical records without an owner-index entry retain their
historical non-held semantics.

Lock has one input, Lock, and no Release input. Its whole attempt takes a
held record, emits Locked, and awaits the end of its connection. The ledger's
AcquireLock translation retains the checked binary exchange in a Held value;
the generated Pay keeps it in a local position through settlement or refusal.
Peer EOF ends the lock attempt and releases its record. The server observes EOF
as an event, with no expiry or retry loop.

An output position emits a framed Signal reply while dispatch is still running.
The runtime suppresses a duplicate final reply. Acknowledge emits Acknowledged,
then runs Tail; a deterministic observer barrier and its trace demonstrate
that the caller already has its answer while Tail is still alive.

## Recovery and evidence

Reserve atomically debits and keeps Reservation. Settle or refund keeps Payment
and removes Reservation. Startup runs the generated Reservation attempt before
accepting inputs: Status, then settle/refund or idempotent Remit if Unknown.
Completed Payments retain duplicate identity but need no lock-release recovery.

The world translator is its own process and redb journal. Atomic journal
insertion is the model's payment effect; a repeated identity returns the
previous result. A real provider needs its own idempotency guarantee.

The kill matrix covers ten boundaries, from before Lock through after Settle,
ledger-primary and world-primary deaths, successful and failed payments where
the boundary is reachable: 36 cases. World-primary cases also stop the ledger;
ledger-primary cases at world-owned barriers also stop the world. Every case
checks removal and reacquisition of the account lock before recovery, correct
balance, specific Duplicate refusal, and exactly one world journal entry.
Before Reserve, no durable request exists: restart first retains the original
balance, then an explicit caller retry pays once. A separate process test kills
the lock owner itself and proves its persisted holding is purged on startup.

## Retained boundaries

Bootstrap still takes one binary LaunchConfiguration file path, not Place/XDG
resolution. Text clients take that path plus datom. Tracing startup waits for
its debugger before opening Memory. Limits and peer paths are configuration;
channel allocations remain startup-only. Configuration persistence/subscription,
full compile-time removal of ordinary trace contexts, provider-side effects
separate from journal insertion, power loss during redb commit, competing ledger
owners, retention and sustained load remain outside this experiment.

Tests stay beside the trial as inherited experiment evidence; separate test
repositories have not been introduced in this pass. All compilation and the
committed-revision Nix gate run on the configured builder.
