# Pass three: author surface trial

`operation-scope` owns the execution library. `trial-ledger` and `trial-lock`
are consumers generated at build time from their four `ethos/` files by
`experiment-upgrade` beside Ethos Zero. No actor crate is in their normal
runtime dependency trees. Earlier passes remain evidence in their own crates.

Each consumer's `build.rs` selects schema files and explicit generator policy,
then emits `OUT_DIR/nexus.rs`. Bodies implement generated vocabulary against
shared traits. Generated code supplies Wire, record identity, fingerprints,
Object/Outcome, Upgrade, historical types, serving dispatch, resumable startup
and the optional ordinary/meta/debug textual tools. `Cargo.toml`, `build.rs`,
`lib.rs`, the ledger's Finishing trait and world-stub scaffolding remain authored
beyond Ethos and bodies; the trial report measures them rather than hiding them.

## Processes and bootstrap

The ordinary Nexus mains are one line. A Nexus reads one binary archived
`operation_scope::LaunchConfiguration` file, whose path is its sole argument.
The generated text client and meta client take that path then one datom; the
debug translator takes that path alone. This is a bootstrap limitation, not
Place/XDG resolution or a datom-only production CLI. The trace feature binds
its trace socket first and waits for the debugger before opening Memory.
The trace-only build needs no text dependencies. Text tools require `datom`.

Configuration carries frame, command, trace and subscription limits, peer paths,
and deterministic failure-boundary settings. Meta changes frame/subscription
limits and peer paths one value at a time. Command/trace channel allocations
are startup-only; attempted resizing is refused. Configuration is not persisted
or subscribed to yet. These are explicit remaining departures from revision 3.

A subscriber receives the initial record and sends a typed `SubscriptionDemand`
for each subsequent read. The owner retains one watch value per subscribed key;
newer commits replace an unread value. This demand boundary prevents a server
from queuing old snapshots in a socket while its peer is not reading.

## Durable world stub

`trial-world` is a separate translator process, with its own redb journal.
Remit and Status greet and cross checked binary sockets. An identity is the
journal's unique key; outcome creation and insertion are one write transaction,
and a repeated call returns that outcome. The bounded model equates the external
effect with this atomic journal insertion. A real payment provider that commits
its effect separately needs that provider's own idempotency guarantee.

Reserve debits and keeps Reservation. Settle keeps Payment (including account)
and removes Reservation. Refund also keeps Payment, retaining identity and lock
recovery. Startup resumes Reservations then completed Payments before inputs;
completed records release orphaned locks after a death between settlement and
lock release. Completed records remain for duplicate detection and are scanned
on restart; this is an intentionally bounded ledger, not a retention policy.

The lock has a generated historical migration from one-field HeldLock to its
current added Integer field. The owner upgrades records and stamp atomically,
and publishes Upgrade's root yield after the enclosing commit. Unknown stamps
refuse before reading records. The ledger currently has no historical chain.

## Evidence and reproduction

All compilation runs on the configured builder. Cargo source probes use the
build-base toolchain. The committed-revision Nix gate is the release gate.
Consumer scenarios cover poisoned nested refusal, serialized roots, partial
transfer rollback, one latest state after 1000 changes (in-process and socket),
commit-failure caller/trace agreement, mismatched greeting, nine controlled
ledger/world death/restart cases, duplicate identities, actual historical-store
upgrade and unknown-schema refusal. The world journal is inspected independently
for exactly one outcome entry. Fault barriers are connection events, not polling.

The commit-failure scenario injects a refusal at the commit boundary, before
redb commit, and verifies rollback and root trace; it is not a disk-failure test.
The process scenarios kill real processes but do not simulate power loss during
redb commit. Debugger disconnect ends a tracing Nexus. Ordinary builds suppress
trace publication, but currently retain generic trace contexts and object
conversions; complete compile-time removal of that machinery remains open.
