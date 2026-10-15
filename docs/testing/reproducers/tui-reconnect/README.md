# Terminal connection recovery

This driver launches the actual CLI/TUI and daemon in isolated XDG directories, selects offline browsing, retains a Unicode draft, stops/restarts its daemon, edits while disconnected, and checks automatic reconnection. It also kills its owned daemon to exercise actual session/WAL recovery. No daemon replies, model output, or UI pages are fabricated. The default workflow requests no inference.

```sh
uv run --with pyte python docs/testing/reproducers/tui-reconnect/replay.py --binary target/release/peritus --output /home/USER/pr-fixed
```

Keep `peritusd` beside `peritus`. Use a new output directory. Run the old binary pair with `--expect-bug` to establish that only manual Ctrl+R recovered it. The fixed program must reconnect without any reconnect keystroke, retain the exact draft, remain interactive during backoff, and log transport failure details without logging the draft. The driver stops only its owned disposable processes.

Optional `--with-retry` selects the real configured ChatGPT account and explicitly chooses
`gpt-5.6-sol`. It submits a short read-only Plan request, waits for visible active work, sends
`/stop`, waits for the observed cancellation and acknowledgement to settle, then types `/retry`.
After that deliberate Retry is admitted, it shuts down its daemon, restarts it, and checks
automatic reconnection to the original run without another input or model request. This mode
requires the real vendor client and account; an unavailable provider is a blocked dependency,
not an invitation to substitute one. The fixture file must remain unchanged.

The recovery oracle uses persistent online UI state, not a fleeting reconnect notice that the
next subscription event may replace. Run records supplement the UI observations; readiness of
a record alone does not establish that its corresponding control acknowledgement is visible.

With `--with-retry --attach-checks 2`, two additional ordinary launcher/TUI windows open the
same active run. Each must reach its conversation within eight seconds with exactly one
generation-1 connection and no failed attempt. Closing these clients must not cancel the run.

`context_load.rs` is a real public context-library consumer, separate from the daemon driver.
After building `peritus-context`, compile it with the matching release rlibs (hash suffixes vary
with the build):

```sh
rustc --edition=2024 -O -C panic=abort docs/testing/reproducers/tui-reconnect/context_load.rs -L dependency=target/release/deps --extern peritus_context=target/release/deps/libperitus_context-HASH.rlib --extern peritus_role=target/release/deps/libperitus_role-HASH.rlib --extern peritus_codec=target/release/deps/libperitus_codec-HASH.rlib --extern peritus_types=target/release/deps/libperitus_types-HASH.rlib -o /tmp/context-load
/tmp/context-load --shape scenarios
/tmp/context-load --shape none --nodes 4096 --repeats 1
```

It also accepts `--snapshot PATH --run RUN_HEX --workspace WORKSPACE_HEX --preferred 4096
--available 200000` for a digest-checked private copy of an actual working-state artifact.
Snapshots require the expected original lineage; private text is never printed. Output includes
counts, full typed-render/ordered-omission digest and timings. Preserve the baseline and fixed
executables separately when comparing a repaired planner. The [handshake campaign](../../handshake-stall-real-2026-10-15.md)
records the actual commands, hashes, independent oracles and bounded results.
