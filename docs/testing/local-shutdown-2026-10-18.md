# Local shutdown recovery — 2026-10-18

## Confirmed defects

The launcher tells a user with changed provider or executable settings to run
`peritus shutdown`. The CLI instead required `--endpoint`, so its advertised
recovery command exited with usage status 2 before contacting the daemon. This
was reproduced with the original installed program and the pre-fix executable.

The first real shutdown campaign also reproduced a concurrency defect: two
shutdown commands reached a running daemon, one completed cleanly, and the other
reported `read daemon frame: early eof`. The endpoint can withdraw before every
client observes completion. Checking ownership immediately after EOF was too
early; the state lock had not yet been released.
Waiting for lock release recovered the outcome, but another schedule took a full
request timeout. The final fix also serializes local shutdowns and launches using
the existing lifecycle lock, so duplicate commands do not compete for one
shutdown reporter. The fallback remains for stops requested through other clients.

## Final behavior and independent expectations

- Plain `peritus shutdown` selects the current user's packaged daemon from its
  live instance descriptor and waits for protocol completion and release of the
  daemon's state lock. It neither launches a daemon nor performs provider setup.
- No daemon, including an installation that has never been initialized, is a
  successful no-op. An absent live descriptor with a held state lock cannot be
  reported as already stopped: the command waits and fails if ownership persists.
- Discovery accepts only a versioned, canonical daemon endpoint name. It uses
  the same native pathname shortening and canonical state root as startup,
  including long paths and XDG directories reached through symlinks.
- Explicit endpoints retain the automation behavior and select that endpoint
  even when a different local daemon is running.
- Connection loss counts as a completed stop only after state ownership ends.
  The JSON result then says `completion_observed: false`, rather than claiming
  to have observed a clean protocol completion.
- Invalid sessions, permission failures, malformed descriptors, protocol errors,
  and daemons that retain their lock remain failures.

These expectations were established from the user-visible shutdown contract,
the reported failure, per-user isolation, and the daemon's documented exclusive
state ownership. The oracle checks real process exit, a separately acquired
OS lock, untouched workspace bytes, and the continued readiness of a neighboring
daemon. It does not substitute a server or copy the endpoint-derivation algorithm.

## Replay

```bash
cargo build --locked --release --jobs 2 -p peritus-cli --bin peritus
python3 docs/testing/reproducers/local-shutdown.py \
  --binary target/release/peritus \
  --output target/shutdown-real/replay
```

The driver uses the actual packaged launcher, terminal client, daemon, status
protocol, and shutdown protocol with disposable state. It generates native,
Unicode/space-containing, shortened long, and symlinked endpoint paths; interleaves stops
and repeated stops; corrupts unrelated settings after startup; operates two live
installations; and launches simultaneous shutdown commands. It also exercises
human/JSON output, explicit endpoints, invalid sessions, and held-lock/invalid
descriptor failures. Its Linux process and socket observations require `ss`.

Use `--baseline <pre-fix-peritus>` to replay the original usage failure as well.
The separately retained structured evidence records build hashes and results.
Raw transcripts and initial/final receipts remain under `target/shutdown-real/`.

One early driver expectation assumed an invalid requested session would be
reported as negotiation status 11. The actual server closes that connection,
which the established client reports as connection status 10. The independent
requirement is rejection while the daemon remains running; the driver was
corrected to exercise that real failure category, with a short explicit timeout.
No product behavior was changed to accommodate this driver correction.

The changed Linux executable was built with two Cargo jobs in a resource scope
with `MemoryHigh=5G`, `MemoryMax=6G`, and `CPUQuota=300%`. No unit tests, mock
servers, formal verification, or repository qualification suites were used.
Native macOS and Windows execution is outside this Linux campaign's evidence.

## Final observed results and installation

[Structured evidence](local-shutdown-evidence.json) binds the six changed Rust
source files, original/final executables, concrete failure captures, and all 29
CLI observations. All 29 met their expectations, including deliberately rejected
inputs and the original status-2 reproduction. Eight disposable daemon instances
were started and stopped. All four simultaneous-stop pairs completed within the
driver's ten-second idle-stop allowance; they no longer wait for the normal
thirty-second request timeout. Temporary installations were removed.

Builder's CLI was replaced atomically with the final executable, SHA-256
`62bbfe21fa4ec1f5e8ee3c74afeff208981cdf50aaf62c75cd4bd5a2f25a7b06`.
The existing daemon then shut down cleanly using exactly `peritus shutdown`;
its process exited, and the original stopped run record was byte-for-byte
unchanged. Repeating the same command returned `daemon is already stopped`.

Two fresh installed-client `peritus resume` operations in builder's project
both reached the actual TUI: the first in 4.68 seconds, the second in 0.32 seconds.
They reused one restarted daemon and retained the original stopped run. The
clients exited with Ctrl-Q; the daemon remains available. No model choice was
changed and no retry or new implementation request was submitted. Previous
binaries and private operational receipts are retained under builder's
`repairs/local-shutdown-20261018` state directory. The daemon executable already
contained the preceding provider-decoder repair and did not need replacement
for this CLI/launcher-only change.
