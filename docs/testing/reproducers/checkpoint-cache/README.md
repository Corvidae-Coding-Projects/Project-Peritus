# Checkpoint and context cache replay

These are ordinary consumers of production APIs and actual processes. They are outside Cargo
membership. Use new output directories and disposable workspaces; never point a replay at a live
user run. Expected source bytes, path changes, process outcomes and entry states come from the
fixture owner rather than from the implementation's proposed answer.

Build the actual daemon with bounded parallelism in a chosen target directory, then link consumers
against that target's dependency cohort:

```sh
CARGO_TARGET_DIR=/tmp/peritus-cache-target CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
python3 docs/testing/reproducers/checkpoint-cache/link-consumers.py --deps /tmp/peritus-cache-target/debug/deps --output /tmp/peritus-cache-consumers
python3 docs/testing/reproducers/checkpoint-cache/replay.py --consumer /tmp/peritus-cache-consumers/scope-consumer --output /tmp/peritus-cache-scope
python3 docs/testing/reproducers/checkpoint-cache/index-invalidation.py --consumer /tmp/peritus-cache-consumers/scope-consumer --output /tmp/peritus-cache-index
python3 docs/testing/reproducers/checkpoint-cache/context-replay.py --consumer /tmp/peritus-cache-consumers/context-consumer --output /tmp/peritus-cache-context
python3 docs/testing/reproducers/checkpoint-cache/invocation-state.py --consumer /tmp/peritus-cache-consumers/context-consumer --output /tmp/peritus-cache-invocation
python3 docs/testing/reproducers/checkpoint-cache/mixed-batch-replay.py --consumer /tmp/peritus-cache-consumers/context-consumer --output /tmp/peritus-cache-batch
/tmp/peritus-cache-consumers/process-consumer /tmp/peritus-cache-process
```

`cohort.py` follows Cargo fingerprints to use the daemon's actual linked feature variants. The
checkpoint IPC probe also needs an existing compatible `peritus-app-client` build in that target;
`cargo build --locked --jobs 2 -p peritus-cli` provides it. Rust, Python 3 and Linux `/proc` are used.

The scope replay creates 64 MiB of deterministic binary data with seed 8419 and checks unchanged,
changed, removed, restored, permission/type/symlink and multi-process cases. For the legacy NUL-page
case, `high-cardinality.py` additionally needs `--baseline-consumer PATH`, compiled against a
separately built original daemon cohort. It creates 16,384 4-KiB pages and the old large SQLite
index. `migration-replay.py` also accepts an original consumer. `cold-index-race.py --fixture PATH`
uses that owned high-cardinality output to append a legacy journal during a substantive cold scan;
it restores that fixture's original journal afterward. Never supply a production fixture.

`provider-consumer CORPUS` makes four authenticated real requests at fixture-only low effort,
keeps the chosen source corpus unchanged, changes the requested marker, and checks all responses
and actual usage. Use a code corpus of about 94 KiB to reproduce the recorded cache measurement.
It requires the official signed-in `codex` on PATH and consumes account inference. It does not
change the application's configured effort. `provider-recovery-consumer CORPUS disabled` checks
the ordinary route; `invalid` checks a real unsupported-model error. `archive-replay.py --consumer /tmp/peritus-cache-consumers/provider-recovery-consumer --corpus CORPUS --output /tmp/peritus-cache-anchor` drives the full real recovery case. Its consumer's `archive` mode waits on
stdin after its first response so an external fixture controller can archive only its own routing
anchor through the official app-server, then continue; the remaining two requests must work and
emit exactly one safe cache-fallback diagnostic. The case that first revealed the offset panic
is described in the campaign report. `CODEX_ROLLOUT_TRACE_ROOT` is an optional official client
wire-capture facility; keep captures private because they contain prompts and model output.

`live-command.py` and `live-memory.py` operate real TUI clients and their own daemons, using
separate XDG directories and a supplied provider-selection state file (`--provider-state PATH`).
They require Python `pyte`, `ss`, and a directory containing the actual `peritus`, `peritus-tui`
and `peritusd` binaries (`--binary PATH/TO/peritus`). The command fixture accepts `--probe` for the
linked checkpoint IPC consumer and `--model gpt-6.1-sol`. Both require signed-in account inference.
They stop only their fixture's owned daemon and client. The command fixture verifies all ten exact
checkpoint preimages/postimages after restart and selectively rewinds one file. The memory fixture
checks distinct provenance, resolution, supersession and exact archived retrieval.

See [campaign evidence](../../checkpoint-cache-real-2026-10-19.md). `observed-results.json` contains
small measured outcomes, with no credentials, private provider state or large runtime payloads.
