# Proactive bug discovery

This inventory distinguishes deterministic regression replay from exploratory campaigns. A green replay job means the checked-in corpus passed. It does not mean a scheduled fuzz, mutation, or native lifecycle campaign ran. Source revision, working-tree changes, exact commands, tool versions, elapsed budgets and individual results accompany each execution under `target/discovery/`.

## Maintained boundary inventory

| Boundary and owner | Observable invariant and oracle | Entry point | Qualification scope and remaining gaps |
| --- | --- | --- | --- |
| SSE framing, C5 | Complete frames preserve data across chunk partitions; invalid or oversized inputs remain bounded | `sse` fuzz target and stable corpus replay | Synthetic bytes and structured partitions; no live provider calls |
| NDJSON framing, C5 | Newline partitioning preserves complete messages and exact-size CRLF behavior | `ndjson` fuzz target and stable corpus replay | Synthetic bytes and partitions; provider-specific JSON semantics remain owner-suite obligations |
| Working context, C6 | Required dependency closure and capacity bounds agree with a small independent oracle | `working_state` target and stable replay | Bounded generated graphs; estimated tokens do not claim exact provider tokenization |
| Provider reducer, C5 | Structured normalized event sequences handle identity conflicts, gaps, fragments, terminal and EOF consistently | `provider_sequence` target and stable replay | Deterministic sequence oracle; replay determinism alone is not a full provider contract oracle |
| Context mutation, C6 | Designated tests reject meaningful capacity/closure changes | Fixed `selection.rs` inventory | Verus macro bodies may generate zero automatic mutants; this is an explicit reachability gap requiring a curated canary |
| Effect receipt mutation, G4 | Replay cannot bind a completed receipt to a different effect | Fixed `EffectReceiptLedger::(begin\|complete\|load)` inventory | Full owning-package baseline and tests; compilation/proof rejection is not behavioral detection |
| Cancellation/recovery mutation, G0 | Cancellation and interrupted shutdown retain their distinct transitions | Fixed `ProductRunService::(shutdown\|resume_interrupted\|cancel\|retry)` inventory | Full owning-package baseline and tests; scheduler exploration and native crash campaigns remain separate evidence |
| Campaign execution, A0 | Missing inputs, timeouts and untested mutations cannot pass; child containment preserves a sibling canary | `xtask` runner and policy tests | Local Linux process-group test; Windows/macOS execution must be reported separately |

Product-run recovery and the journal-backed AgentDriver have separate owning test suites and durability contracts. Process recovery manifest decoding stays private and is exercised in its owner crate. Native package checks belong to the existing platform qualification jobs; synthetic fuzzing does not qualify Windows locked-file behavior or native supervisor state.

## Replaying regressions

Populate the candidate's locked dependency cache once with `cargo fetch --locked`. Then run:

```text
cargo xtask discovery-replay
```

This command validates that all four fuzz binaries and `discovery-replay` are registered and every corpus contains regular files of at most 8192 bytes. It runs each corpus through the stable binary with `--locked --offline`, at most 110 seconds per target. A missing target, empty corpus, failed assertion or timeout is an error. Individual ordinary replay is:

```text
cargo run --locked --offline -p peritus-bug-discovery --bin discovery-replay -- sse crates/app/testing/peritus-bug-discovery/corpus/sse
```

Change both target and corpus suffix for the other registered targets. Preserve raw input bytes, target name and minimized regression together.

## Bounded discovery

Production Rust remains pinned by the existing workspace toolchain. The explicit setup commands install harness tools; PR replay never invokes them:

```text
cargo xtask discovery-setup-fuzz
cargo xtask discovery-setup-mutation
```

Fuzz setup installs `nightly-2026-08-09` with `rust-src` and `cargo-fuzz 0.13.2`; mutation setup installs `cargo-mutants 27.1.0`. No setup command changes the default toolchain. A native C++ compiler is required by libFuzzer. The engine invocation uses `--no-cfg-fuzzing`, so production validation cannot be bypassed by cargo-fuzz's usual global cfg.

```text
cargo xtask discovery-fuzz-sse
cargo xtask discovery-fuzz-ndjson
cargo xtask discovery-fuzz-working-state
cargo xtask discovery-fuzz-provider-sequence
cargo xtask discovery-mutation-context
cargo xtask discovery-mutation-receipt
cargo xtask discovery-mutation-cancellation
```

Each fuzz campaign copies the corpus into its evidence directory, uses seed 881, an 8192-byte maximum input, 2048 MiB RSS limit, 30-second per-input timeout and 120-second engine budget. The complete build/run process has an eight-minute bound. A success exit also requires the engine's completion log to demonstrate nonzero executions and its full allotted budget. Build failure, absent completion evidence, timeout and oracle failure cannot become successful campaigns.

Cargo-fuzz 0.13.2 has no locked-input forwarding switch. The wrapper first resolves metadata with `--locked --offline`, disables Cargo network access during fuzzing and compares the complete lockfile before and after execution. A changed lockfile invalidates the campaign and remains visible for investigation. Checked-in corpora are never modified by the engine.

Mutation commands first retain `--list --json` with exact source ranges and diffs. Zero discovery is an error classified as `unreachable`, not test effectiveness. Nonempty inventories run a clean unmodified baseline and the same full package suite for each mutant in isolated copies, with denied lints retained, locked offline Cargo, one mutation worker and two compiler jobs. For a fixed eighth of the full inventory, append `-0` through `-7`, for example `cargo xtask discovery-mutation-receipt-0`. The wrapper retains both full and selected inventories, an explicit outside-this-shard count, and selected outcomes; an empty shard fails. Scheduled mutation jobs cover all eight shards with at most two running concurrently. Eight minutes bounds the whole campaign; build and test phases have additional limits. All discovered mutants must have exactly one recognized outcome before completion can be reported. Partial inventories retain an `untested` count. `caught`, `missed`, `unviable`, `timeout`, unclassified outcomes and baseline failure are separate; equivalence requires independent review. No mutation coverage score is inferred.

## Minimization and accepted failures

Preserve the original crash artifact and failing logical signature before minimizing:

```text
cargo +nightly-2026-08-09 fuzz tmin sse target/discovery/CASE/crash-INPUT --features fuzzing --no-cfg-fuzzing --fuzz-dir crates/app/testing/peritus-bug-discovery -- -max_total_time=120 -rss_limit_mb=2048 -timeout=30
```

`CASE` and `crash-INPUT` are placeholders for the reported evidence path. Replay the minimized input through `discovery-replay`; reduce structured operations or fault points only while retaining the same oracle failure and required reachability. A minimized byte input that no longer reaches the original boundary is not an accepted reproducer. The owner must retain an ordinary regression that fails before the fix and passes afterward. Accepted chaos findings additionally require three independent fresh-fixture reproductions, persisted state observations, and a final process/filesystem census.

## CI status and evidence

The checked [workflow](../../.github/workflows/bug-discovery.yml) runs stable replay for pull requests. Weekly and manually dispatched campaigns use one Linux job per fuzz target and mutation slice/shard, with a ten-minute ceiling and failure-independent shards. The policy test audits the exact inventory, toolchain, pre-Cargo configuration check, commands, trigger conditions and unconditional artifact upload. Missing artifacts are errors. Host job timeouts can interrupt setup or builds before the discovery engine begins; that remains incomplete work, never a clean campaign.

Every operation writes `completion.json` as `incomplete` before launch. Each child also records its invocation before launch and a terminal command result afterward. Engine logs, mutated-source inventories, private corpus and deliberately retained scratch storage remain under the operation's evidence directory. The runner owns a fresh Unix process group or Windows Job Object, terminates the owned group on timeout and after root exit, and reaps its direct child. The Linux qualification checks that a reached descendant is no longer executing and a separately owned sibling survives; it does not claim arbitrary detached-process containment. Final accepted-failure evidence must include any additional native census required by the campaign.

Local infrastructure tests are execution evidence only for their actual host. Hosted job timing and non-Linux containment remain unverified until those jobs run against the delivered revision. A shard that times out remains incomplete even if another shard passes; aggregate all eight inventories before describing a whole slice as completed. Do not reduce the inventory to make a pass appear complete.
