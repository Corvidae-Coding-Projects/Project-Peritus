# AgentDriver recovery and accounting campaign

- Source base: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Execution host: Fedora Linux, x86_64
- Seed families: transition identities 11-78 and budget identities 1-66
- Commands: `CARGO_BUILD_JOBS=2 cargo test -p peritus-agent --test runtime_driver --test runtime_budget`

## Executed boundaries

The scripted provider tests preserve logical event order independently of timestamps. An exact duplicate provider event is durably counted but does not advance the semantic cursor twice; restoring from SQLite produces the exact same state, cursor 3, and four observed provider envelopes. A sequence gap after cursor 1 returns a model protocol error, and the explicit terminal failure restores byte-for-byte with cursor 1.

The resumable-provider case persists a two-envelope prefix, drops the process-local driver and provider session, reopens the journal, requires an in-flight recovery report, schedules an exact-cursor retry at 2, and accepts only envelopes 3 and 4. The terminal cursor is 4 and the interrupted attempt is conservatively finalized before the retry budget begins.

The lost-tool case persists `ToolDispatched`, drops the driver, and restores one outstanding ordinal. Classification appends exactly one journal event and produces `Indeterminate`. A second classification is rejected and leaves the journal count unchanged, providing the negative control that no classification path can redispatch the effect. Result recording retains the unresolved-indeterminate state.

Budget accounting separately rejects a cumulative interim observation that moves tokens, cost, and active time backwards. The reservation remains active, its high-water usage is unchanged, and the independent B1 ledger consumed total is unchanged. Existing cases also verify exact final usage, retry attempt charges, conservative ambiguous finalization, and tool-effect time.

## Classification and limits

All executed cases passed; this campaign confirmed no AgentDriver defect. The provider and dispatcher are deterministic owner fixtures, while durability uses a real temporary SQLite journal. Temporary directories are RAII-owned and the campaign starts no child process, network service, or external effect.

The crash boundary is process-local owner loss through dropping the driver and reopening durable state. An actual child-process kill was not added because the current integration fixture does not expose a cross-process command protocol; adding one only for this campaign would widen the private runtime surface. SIGKILL and power loss remain unexecuted. ProductRunner command receipts are a separate engine and were not used.
