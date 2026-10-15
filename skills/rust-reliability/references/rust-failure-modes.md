# Rust failure modes and structural treatments

Use only the entries relevant to the changed behavior. Consult the documentation for the project's pinned versions when API behavior or lint support matters.

| Failure mode | Specific mechanism | Structural treatment | Required behavioral evidence |
| --- | --- | --- | --- |
| Pending message lost | A cancellable send owns the message; dropping the send drops it | Keep ownership outside the cancellable wait; reserve capacity before transfer; define retention/rejection policy | Force a full queue, cancel the wait, and account for each message identity under the actual contract |
| Partial I/O lost or duplicated | A convenience read/write future contains progress that disappears when dropped | Store progress in an owner that survives the wait; use resumable operations; frame the protocol explicitly | Use a real slow peer or stream, interrupt after some progress, and inspect exact bytes or decoded records |
| Shared state inconsistent after cancellation | Mutation breaks an invariant before an await; dropping the future unlocks or releases state before restoration | Prepare then synchronously commit, retain a valid pending state, or give completion/recovery to a supervisor | Coordinate interruption during the vulnerable interval; inspect state through another real caller |
| Cleanup abandoned on first error | A try-join or early return drops unfinished cleanup futures | Drive required cleanup to completion before aggregating errors; specify deadlines and forced stop | Make one component fail and another finish slowly; observe terminal states of all required components |
| Detached worker failure hidden | A dropped Tokio JoinHandle detaches its task; result is never observed | Retain handles, inspect nested Result/JoinError, and define restart or failure propagation | Trigger the actual worker failure and check the supervisor's observable reaction |
| Supervised work aborted accidentally | Dropping a Tokio JoinSet aborts remaining tasks; runtime shutdown also drops pending tasks | Tie task-set lifetime to an explicit supervisor; drain before runtime destruction | Exercise shutdown and caller exit while work is active; inspect accepted-work outcomes |
| Panic corruption reused | Unwinding exits during a multi-step mutation and another caller reuses the data | Roll back, retain a valid sentinel, or invalidate and rebuild; handle poisoning deliberately | Force a reachable failure mid-transition and inspect recovery through the normal consumer |
| Mutex replacement loses detection | Replacing a poisoning mutex with a non-poisoning one removes an indication of possible corruption | Preserve detection where needed or provide equivalent invalidation/recovery | Exercise a panic and verify the expected failure or recovery behavior |
| Executor stalled or deadlocked | Blocking operations run on runtime threads; cyclic lock dependencies; locks held through async calls | Keep critical sections short; establish lock order; use appropriate blocking isolation or a single state owner | Run concurrent real operations with stalled peers and bounded completion expectations |
| Unbounded memory growth | Unbounded queues, detached tasks, repeated retained clones, missing retention limits | Establish backpressure and explicit resource budgets; observe rejected or retained work | Sustain overload through the real entry point and inspect queue depth, work accounting, and memory against agreed limits |
| Future never executed | Future constructed then ignored, including `let _ = future` | Enforce unused-must-use and discarded-future diagnostics; make intentional drops explicit and reviewed | Exercise the caller and observe the required effect; a startup success is insufficient |
| Lock dropped immediately | `let _ = lock()` drops the guard at the statement boundary | Enforce dropped-lock diagnostics; use a named binding for intended lifetime | Exercise the shared-state workflow under relevant concurrency |
| Unsafe abstraction trusts safe callbacks | Safe Ord/Hash/Iterator/Read/Drop behavior is erroneous, stateful, or panics; unsafe code assumes its lawfulness | Separate memory-safety invariants from callback promises; use checked internal identities and safe fallbacks; narrow unsafe boundaries | Build a real public-API consumer with pathological permitted callbacks; exercise legal operation sequences |
| Compiler workaround hides design defect | Transmute, lifetime extension, unsafe Send/Sync, memory leaks, excessive Arc/Mutex, ignored Result | Explain the actual ownership requirement; use private typed APIs and a supported ownership design | Operate the affected behavior and relevant failure paths; treat style findings as advisory |

## Cancellation facts that require precise reasoning

- A Rust future is driven by polling. Dropping a partially polled future cancels further execution of that future; it does not roll back completed external side effects.
- Tokio `select!` drops branch-owned futures when another branch wins. If a branch borrows a future retained outside the selection, dropping the borrow does not drop that retained future. Determine actual ownership before diagnosing loss.
- A timeout can cancel its owned inner future. Timing out while awaiting a retained task handle does not necessarily stop the underlying task; check the actual API and ownership.
- Tokio MPSC `send` can lose its owned message on cancellation. `reserve` keeps the payload out of the pending wait, but cancellation can lose queue position. A successful send does not prove that a receiver processed or durably stored the message.
- Common convenience operations such as `read_exact` and `write_all` can lose partial progress when cancelled. Lower-level operations still need a caller that retains offsets/buffers and honors the protocol.
- Cancellation safety of an individual operation and correctness of the enclosing system are different obligations. The system contract determines whether discarding progress is permitted.
- Single-owner actors can still fail while processing, leave external effects half completed, or lose in-memory accepted work on shutdown. Use the actual durability and recovery contract.
- Poisoning detects some panic-related corruption; it neither restores state nor proves safety. Async cancellation does not automatically poison a lock.
- `panic = 'abort'` tears down in-process state but does not roll back external effects or establish crash recovery. Do not prescribe it as a universal repair.
- Private typed states constrain callers only within their scope. Deserialization, unsafe code, FFI, exposed mutable representation, and unverified boundaries require separate treatment.

## Unsafe boundaries

Do not use safe callback promises as the sole basis for memory safety when callers can violate them in safe Rust. State every safety obligation and its enforcing mechanism. Review both normal return and panic/cancellation paths.

Use a minimal consumer of the real public API for adversarial traits and sequences. Where the project already supports Miri, it can supply additional execution evidence for supported paths; do not present a Miri pass as proof over every input or require a unit-test suite. Treat unavailable Miri/FFI support as a scoped limitation. Do not fabricate soundness defects: reproduce the claimed violation or report it as unconfirmed.

## Primary references

- Rain, Cancelling async Rust: https://sunshowers.io/posts/cancelling-async-rust/
- Rain, Dealing with cancel safety in async Rust: https://rfd.shared.oxide.computer/rfd/0400
- Rain, In defense of lock poisoning in Rust: https://sunshowers.io/posts/on-poisoning/
- Rain, iddqd, or the hardest kind of unsafe Rust: https://sunshowers.io/posts/iddqd-unsafe/
- Tokio select and cancellation safety: https://docs.rs/tokio/latest/tokio/macro.select.html
- Tokio MPSC Sender: https://docs.rs/tokio/latest/tokio/sync/mpsc/struct.Sender.html
- Tokio JoinHandle: https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html
- Tokio JoinSet: https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html
- Clippy lint reference: https://rust-lang.github.io/rust-clippy/master/index.html
