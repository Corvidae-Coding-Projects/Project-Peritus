# Binary evidence in model text: live failure and repair

Builder's retained run `59ed85009de6c149d98e99b65e888b72` stopped with
`execute D0 developer loop: text: model text is empty, contains NUL, or exceeds its byte bound`.
The daemon remained available. The latest provider response was valid, short completion JSON
after a successful real PostgreSQL/filesystem journey; the following host-generated input failed.

## Confirmed cause

The saved diff was 524307 UTF-8 bytes containing 208306 literal NUL bytes, SHA-256
`b9a669ee3893d3ead5969469523df5df6eac3f32144822d52227737631f16cb5`. Its first NUL
was a preview of the 8192-byte PostgreSQL page
`.prescient/acceptance-runtime/postgres/base/1/2600_vm`. Gates and the completion summary
contained no NUL. The model text ceiling was 16 MiB; this failure was invalid text, not size.

Zero-filled binary pages can be valid UTF-8. The in-place renderer accepted those pages as
text and embedded their preview bytes verbatim. Model message construction correctly rejected
the NUL-containing text. This was a separate boundary from the earlier runtime-churn repair.

## Repair

In-place rendering now labels NUL-bearing before/after previews as binary without changing
durable stamps, baseline equality, or full-file fingerprints. This also handles previews
written by previous versions. Git untracked-file rendering applies the same distinction.

Shared developer/reviewer evidence projection handles already-saved NUL-bearing sections with
a bounded omission marker. The marker identifies the original byte count, SHA-256, and exact
omitted range; the immutable source remains available through ordinary paged evidence reads.
Marker size is included in context allocation. Ordinary source text is unchanged, and the
model protocol continues rejecting raw NUL text.

## Real replay evidence

Before the repair, an independent public consumer reproduced four failures: a newly created
8192-byte binary page in a scoped diff, projection of that diff, a short NUL-bearing historical
diff, and NUL-bearing process evidence. Ordinary Unicode text and the raw-text rejection
invariant passed. No provider was mocked and no unit-test framework was used.

Private raw observations and the original/fixed consumers are retained under
`/home/doll/Peritus-Builder-Repair/text-input-followup/`. The production build uses:

```sh
cargo build --locked --release --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
```

The [retained replay guide](reproducers/binary-evidence/README.md) includes the real consumer,
fixture setup, terminal driver, independent expectations, and exact replay commands. Private
user diffs and credentials are not bundled with those resources.

The final consumer passed 22 scenarios, including the exact saved live diff, NUL at distinct
positions in 1/65/8192-byte inputs at two context targets, and safe rendering of NUL-bearing
stored baselines. Mutating a binary byte beyond the 16 KiB preview still changed its observed
identity; restoring its original bytes restored its baseline. Exact source reads, original
byte counts, independent SHA-256 values, and reconstruction through JSON-encoded pages all
matched. The saved live diff reconstructed through 402 small pages. The ordinary Unicode
prompt remained byte-for-byte identical to its original version.

All 24 existing in-place scope journeys and their 40 concurrent runtime observations also
passed against the final library. No unit tests, mocked providers, formal verification,
Clippy, or repository qualification suites were run.

The actual TUI/daemon/Codex workflow in a fresh disposable Python project completed in
180.565 seconds and reached Qualified at cycle 1: two changed files, four successful
acceptance commands, and an independent review with no findings. The output page contained
exactly `bytes([0,65,0,66])*2048` (8192 bytes and 4096 NULs); independent execution of
`python3 main.py` returned exactly `SOURCE-REPAIRED` plus newline, exit 0. The candidate diff
contained no NULs. Actual Reviewing requests 12 through 15 followed the last Writing request
11, proving that the workflow crossed the original post-writer input boundary.

An initial disposable UI fixture without a project manifest stopped at its missing check
contract; that outcome is retained as blocked, not as reproduction of the NUL failure. The
successful fixture supplied an ordinary dependency-free Python project manifest. The first
driver verdict also missed the brief Checking phase between half-second polls. Its original
result is retained; the corrected oracle uses actual subsequent Reviewing requests and the
Completed/Qualified record, rather than requiring a sampled transient phase. The raw product
observations and binary/source oracles independently establish the successful workflow.

## Builder installation

The final daemon SHA-256 is
`c92a60b8222af5334e6d9854c39b605f4fb1cfc468df546d91acdac0faf30e27`.
CLI and TUI hashes remain
`bd81ef6090578893810d162e2d427cec40a321d74f08f013122230b3444db353` and
`7429ced8bed358b9d30e342db444a2a2dac3500edcc34e80a16767cf44e3c6aa`.
All installed binaries retain builder ownership and mode 0755; the running daemon's
`/proc/PID/exe` digest matches the final build.

Normal shutdown reported `remaining=1`; independent inspection confirmed the old daemon
and its owned PostgreSQL processes had exited before backing up closed state. The prior
binaries, durable state, and configuration are retained in builder's private
`repair-backups/binary-evidence-20261016-063921` directory. Replacement used the launcher
lock and atomic file replacement. No force kill, state discard, or acknowledgement of
unknown effects was used.

The normal installed launcher reconnected to the original run on its first attempt in
0.432 seconds. Retry answered a newer read-only architecture question in that conversation
and entered WaitingForUser. This is a successful conversational reply, not evidence that
the full application implementation or its outstanding acceptance work has completed.
