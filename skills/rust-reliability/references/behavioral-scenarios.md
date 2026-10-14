# Rust scenarios for real testing

## Common execution procedure

Define expectations from the requirement before operating the program. Build and launch its current production path with real local dependencies and isolated data. Use a CLI/API/UI/worker input or a real consumer of the public library API. Observe output and downstream state, then read through a fresh client or restart where the contract promises persistence.

Choose a bounded exploration budget based on relevant dimensions. Generate reachable valid inputs and legal operation sequences as well as targeted invalid inputs. Retain seeds, concrete actions, and environment. Confirm generator/oracle correctness before treating a mismatch as an application bug. Minimize and preserve real failures, fix them, rebuild, and replay. Keep oracles structurally independent from the production algorithm. Mark unavailable dependencies or inaccessible fault points explicitly; never substitute a mock and call the real path verified.

## Scenario selection

| Trigger | Execute | Inspect independently |
| --- | --- | --- |
| Queue admission or timeout changed | Fill the real queue, begin identified submissions, establish that they are waiting, then cancel or expire them and release capacity | Each ID's accepted/rejected/retained outcome; exact contract-defined accounting; no unexplained disappearance |
| Accepted work survives client exit | Submit identifiable work, observe the actual acceptance point, disconnect the caller, then reconnect | Work state and destination effects; recovery after restart if promised |
| Shared transition spans an await | Coordinate a slow real dependency so the transition reaches that await; cancel the caller or supervisor as appropriate | State through another real caller; allowed pending/error states; absence of prohibited intermediate state |
| Streaming protocol changed | Use a real peer that sends/receives records in fragments; interrupt after observed progress; reconnect/resume when supported | Exact records/bytes, offsets, ordering, and duplication according to the protocol |
| Cleanup/shutdown changed | Keep multiple workers active; induce a legitimate error in one cleanup while another is still pending; request shutdown | Every component's required terminal state; documented deadline; accepted work and child-process state |
| Worker failure/recovery changed | Induce a reachable worker error or panic using supported fault injection or a real failing dependency | Supervisor-visible failure, restart policy, work ownership, recovery, and durable state |
| Idempotent retries promised | Reissue the same operation ID through the real entry point before/after an interrupted response and restart | Number and identity of effects, returned state, and durable deduplication behavior |
| Overload/backpressure changed | Sustain traffic above available capacity with identified work and slow real consumers | Bounded resources under the agreed budget, explicit admission outcomes, responsiveness and shutdown |
| Lock/ordering/concurrency changed | Generate concurrent legal operations with distinct values and reversible/stalled peer behavior | Contract-grounded state invariants, completion/deadlock deadlines, routing and identity |
| Unsafe generic abstraction changed | Build a small real consumer with legal pathological callback implementations and operation sequences | Safety invariants and results; supported Miri execution as additional evidence where available |

## Make failure injection meaningful

- Place barriers in a legitimate local integration driver or use an existing controlled fault facility. Preserve the real feature path. A driver that implements the missing feature itself cannot validate the feature.
- Distinguish cancellation before polling, while waiting for capacity, after acceptance, during mutation, and during recovery. Exercise only points relevant to the changed contract.
- Do not use an arbitrary delay as proof of reaching a vulnerable interval. Observe an event or state, use synchronization, or report the timing uncertainty and reproduction frequency.
- Kill a process only when crash recovery is in scope. Cancellation, panics, graceful shutdown, and process death exercise different mechanisms; do not substitute one for another.
- Do not insist on no duplicates or no loss unless the actual contract promises it. For at-least-once delivery, check deduplication only if specified. For best-effort telemetry, dropping records may be legitimate.
- Keep the expected contract stable during repair. Change an erroneous oracle only with evidence from the requirement or an independent source.

## Compact scenario record

Record the following in the project's existing evidence format:

```text
Scenario ID and requirement:
Candidate/build identity and configuration:
Real interface, dependencies, and isolated setup:
Actions / concrete input / causal coordination:
Independent expected observation:
Observed output and downstream readback:
Status: PASS | FAIL | BLOCKED | NOT RUN
Exploration seed, budget, actual cases and dimensions:
Failure reproduction and clean-state replay command:
Final result after fix and rebuild:
Unexercised boundary:
```

Keep each retained failure as a small concrete input/action sequence with fixtures and prerequisites. A seed alone is insufficient. Preserve useful generators for future fresh exploration; avoid large success transcripts and redundant examples.
