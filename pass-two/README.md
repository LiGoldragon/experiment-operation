# Operation experiment, pass two

Implements the bounded revision-2 trial from Fable's design of 19 November 2026. This is an experimental machine, with production Nexus integration left visible.

## Anatomy

- `lib.rs`: the trait contracts; `Changeable` bodies are synchronous and receive only `Transaction`; `Operable` bodies are async and receive `Scope`.
- `ledger.rs`: Account, Reservation and the shared change/attempt bodies. Transfer runs both child changes in one transaction. Reserve persists its debit and reservation together; Settle removes the reservation; Release refunds it. A duplicate outstanding reservation is refused.
- `memory.rs`: redb records and transactions. A dedicated OS thread owns the store, outside the Tokio worker pool. Each root change occupies that owner through its whole read/modify/write/commit. Any nested refusal poisons the whole transaction, including when a caller catches it.
- `scope.rs`: owner-thread dispatch, plain task attempts, startup Resume and the in-process stub translator. Its Never mode records a successful external result, then loses the reply forever. Cancelling the attempt leaves a Reservation; reopening asks the configured world journal and settles/releases it. Resume and its journal semantics are hand-written.
- `upgrade.rs`, `schema/`, `build.rs`: the compiled fingerprint chain, generated historical types/conversions and the generated Upgrade change. Startup executes it on the same store owner before Resume and before readiness. [Upgrade details and bounds](UPGRADE.md).
- `transport.rs`: pure binary ordinary/meta/trace socket frames and the subscriber holder. Both holder variants run the same state transitions: one plain Tokio task or one Kameo actor, each with an unbounded mailbox. Snapshot registration is serialized with changes by the store owner; only committed final account states are announced, once per affected account per transaction.
- `trace.rs`: closed Object/Outcome enums, root/path assignment and RAII Abandoned checkpoints. These enums and traits are authored Rust, not Ethos-generated. Entered/Yielded, subscription Emitted/Ended and cancellation are traced. The trace socket carries length-prefixed, checked little-endian unaligned rkyv archives. Its empty first frame acknowledges subscription installation; it then streams new checkpoints, without replaying history. Lag closes the connection.
- `operation-debug`: a separate process that validates trace frames and uses datom-codec/protos to print them. Text escaping is the codec's, not a trial printer's.

## Features and execution

`cargo test -p operation-ledger --features trace` runs the complete trial; `--no-default-features` tests the runtime library without trace or tool/text dependencies. Build tools still compile with text support. The `datom` feature enables the manifest tool, the local driver and generated text derives; it is default for executable packaging. `trace` implies `datom` and adds the debugger. An ordinary runtime library holds no trace types, counters, history, socket code or textualization calls; the upgrade's typed result remains ordinary data. Both holder implementations and Kameo are compiled together in this trial.

The driver takes one datom. Example commands, with disposable absolute paths:

```sh
operation-ledger 'Seed./tmp/ledger-store'
operation-ledger 'CrashPay./tmp/ledger-store'
operation-ledger 'Resume.{ /tmp/ledger-store True }'
operation-ledger 'Serve.{ /tmp/other-store /tmp/ledger.sock /tmp/ledger-meta.sock /tmp/ledger-trace.sock False }'
operation-debug /tmp/ledger-trace.sock
operation-ledger 'HolderBenchmark.{ False 10000 }'
operation-ledger 'Benchmark.{ /tmp/benchmark-store False 1000 }'
```

Seed creates Alice at 12. CrashPay exits 77 after reserving 5 and losing the world reply. Resume's Boolean supplies the stub world's result across the process boundary: True settles at 7; False releases back to 12. This exercises actual process death and store recovery, not a separately running durable payment provider. Serve starts a fresh machine and handles typed `Input`/`Output` on the ordinary socket; `MetaInput` reads configuration or stops it. A subscription sends the current Account then each committed final Account until the connection closes. Socket paths and store path are supplied by the driver. The timer stub sleeps once, archives and validates a TimerFired input, then runs it as a new root. It is event-driven, with no polling.

## Bounded claims

This is not the generic Nexus/Signal routing implementation: no peer greeting/fingerprint handshake, caller envelope, generated wire crate, place resolution, meta Configure or general Ask is supplied. The timer and Remit/Status stub live in-process; they are not a separate translator process. Timer input is checked binary data, but its in-process delivery is not a remote peer interaction. The debugger is separate.

The subscriber policy is every committed final state in an unbounded queue. Slow-subscriber resource limits and delivery acknowledgements remain undecided. The trace queue capacity is currently a hardwired 4096; it belongs in runtime trace configuration in a separate configuration release. Tracing retains an unbounded in-memory history for the trial assertions. Failed commits/power loss, world statuses that remain uncertain, competing Resume/Pay ownership, repeated payment identifiers after settlement and root throughput under load are not established by this trial. The subscription benchmark compares mechanisms within the same compiled crate, not separately minimized binaries. Malformed connection frames close that connection rather than returning the complete production refusal vocabulary.
