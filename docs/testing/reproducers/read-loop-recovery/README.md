# Actual read/recovery regression replay

The consumer drives production `WorkspaceDeveloperTools`, `MemoryTools`, `LocalContextHandle`, provider projection, on-disk journals, and process-store configuration. No unit tests or provider substitutes are used. Library-only cases inspect submitted provider messages without requesting inference. Live mode runs the actual official account provider and ProductRunner.

Requirements: the pinned Rust toolchain and cached Cargo dependencies, Python 3, and the normal Codex executable discoverable by Peritus. Live mode additionally needs an authenticated account. Keep output and Cargo artifacts outside the checkout and bound CPU/memory to available resources.

```sh
python3 docs/testing/reproducers/read-loop-recovery/build.py \
  --output /tmp/peritus-read-consumer --target /tmp/peritus-read-build
python3 docs/testing/reproducers/read-loop-recovery/replay.py \
  --consumer /tmp/peritus-read-consumer/consumer \
  --output /tmp/peritus-read-replay --seed 20261020
python3 docs/testing/reproducers/read-loop-recovery/review_evidence.py \
  --consumer /tmp/peritus-read-consumer/consumer \
  --output /tmp/peritus-review-evidence
python3 docs/testing/reproducers/read-loop-recovery/independent_archive_policy.py \
  --consumer /tmp/peritus-read-consumer/consumer \
  --output /tmp/peritus-archive-policy
```

The output directory must be new. `build.py --cargo-output /path/to/build.jsonl` can link a previously built exact Cargo cohort without rebuilding. Build flags select one Cargo worker and omit dev debug information. Optional host resource controls can be placed around the command.

The replayer establishes expected fixture bytes before execution. Each case records input NDJSON, actual results, stderr, workspace fixtures, and private runtime state outside the repository. `results.json` records the consumer SHA-256, seed, actual action count, and PASS cases. A mismatch exits nonzero and preserves the failing sequence.

The baseline is develop `a392c61fcfa596cb97e1b88d79822a124a03b480` (0.1.7). Actual baseline failures included external bodies replaced with false stale notices, next-action prose lost at invocation rollover, 54 inspections cycling through 18 directories with no feedback, 30 identical context reads with no feedback, host warnings pinned as user requirements, and a near-1-MiB file converted to an untyped raw JSON prefix. Baseline raw evidence was retained outside the checkout; the checked-in replayer fails if these defects return.

The generated campaign varies path aliases, UTF-8, overlapping ranges, byte/page limits, file size, invocation rollover, and pressure on inline history. The tests require valid current evidence to remain usable; they do not prohibit a refresh after changes, eviction, permission changes, or a new substantive question. Current user instructions retain authority. Host feedback and prior assistant prose never establish an effect or grant permission.

`review_evidence.py` checks actual public evidence-reader pages against fixture bytes, then submits a real prepared context view. Merely archiving raw output must not grant delivery credit. Same-task coverage survives rollover and cold reopening; another role or changed immutable digest gets separate coverage.

`consumer.rs` also accepts a spec with `live:true`, optional `converse:true`, `model`, `deny_process`, and `deny_network`. That mode calls the actual production pipeline. `restore_existing:true` reopens a disposable copy of completed role memory without introducing another invocation. Use a fresh disposable root, host-private trace directory, and a task with explicit independently checkable effects. Never point a replay at a live user's run.

The authenticated live replay makes real provider requests and has a 900-second external observation deadline. Peritus workspace Process and Network capabilities are disabled; provider inference uses the configured official account adapter. After acceptance, the external fixture owner builds and runs the resulting CLI as the independent behavioral check. It also checks the actual retained read before a temporary file's deletion, without recreating the file. No unit or qualification suite is run.

```sh
python3 docs/testing/reproducers/read-loop-recovery/live_replay.py \
  --consumer /tmp/peritus-read-consumer/consumer \
  --output /tmp/peritus-live-read-recovery
```

See [the campaign report](../../read-loop-recovery-real-2026-10-20.md) for all defect dispositions, exact tested identities, resource controls, actual counts, and limits. [The independent oracle](independent-README.md) explains the private-source reproduction and its original failure.
