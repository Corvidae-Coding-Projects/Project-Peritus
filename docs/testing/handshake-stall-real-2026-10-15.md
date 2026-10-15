# First-connection timeout during long-history prompt planning

## Observed failure and contract

After the initial terminal reconnect repair, the user's restarted client logged two consecutive
eight-second handshake timeouts, then connected on generation 3. The daemon and coding run were
alive. Read-only `peritus --endpoint … status` requests independently took 14.934 seconds and
then 0.004/0.003 seconds. A syscall-traced repeat took 14.654 seconds; Unix connect returned in
74 microseconds, followed by a 14.645-second wait for the daemon. This was a real intermittent
daemon response stall, not evidence that the endpoint was absent.

The contract for this repair is: normal prompt preparation must preserve all selected context
and not occupy the async workers serving connections. A fresh terminal must attach on its first
attempt during an active run, within its existing eight-second deadline. Recovery, Unicode draft
retention, explicit Retry identity and no duplicate Retry must continue to work. No timeout,
context limit, permission, role or provider setting is relaxed.

## Cause and implementation

An eight-second, 49-Hz user-mode profile of the live daemon sampled context selection as its CPU
hotspot. The installed release is stripped; an independent operator matched 25 exact machine
code bytes to an unstripped build and resolved `peritus_context::selection::plan::select_context`.
Original virtual offset `0x692a70` mapped to `0x6689f0` in that diagnostic build; direct reuse of
the original address in a differently laid-out build would have been invalid.

`dependency_closure` made N full passes over all N graph nodes for each selected root, even for
an isolated node with no dependencies. The required/optional selector calls it repeatedly, and
normal certificate construction repeats the selection. Required-working-state headroom can
also call selection again as the allocation grows. Meanwhile coding-run futures performed this
synchronous computation directly on the daemon's async executor.

The repair changes the two relevant ownership/algorithm boundaries:

- Dependency closure uses a worklist that visits each reachable node once, with an immediate
  singleton closure for nodes without dependencies. Results remain in canonical graph order.
- Exact graph lookups use binary search after the existing strictly increasing-ID validation.
- Each coding run executes through an owned blocking worker and the existing runtime handle.
  The retained task join and existing cancellation tokens still own its lifetime; provider and
  process I/O remain on the runtime. Synchronous planning/file/tool work does not occupy IPC
  async workers. No extra provider process, per-run runtime or detached worker is introduced.

Required-first admission, complete closures, atomic optional omission, hidden-dependency
diagnostics, accounting, certificates and all stored state remain in place. No migration is
needed. The earlier automatic reconnect and private diagnostic changes remain applicable.

## Actual brownfield replay

Read-only inspection captured the exact immutable working-state artifact referenced by the
live run's checkpoint. The copied bytes were digest-checked and replayed through the real public
`decode_working_state` and `render_working_state_with_headroom` interfaces. No model output or
application implementation was substituted. Private source text stays outside Git.

Snapshot SHA-256: `fa3254d836bbc839a5876515e4ee72caec0999267438ed76b5c8cdb9f86c3419`;
2,882,631 bytes, state revision 68,263, 466 entries, 463 stale entries, no dependency edges.
The original captured validation's model revision is 1,512. Preferred allocation: 4,096 tokens;
available ceiling for the replay: 200,000 tokens.

| Observable | Original library | Fixed library |
| --- | ---: | ---: |
| Decode | 5.474 ms | 5.791 ms |
| Actual snapshot render | 10,571.811 ms | 4.010 ms |
| Selected / omitted entries | 463 / 3 | 463 / 3 |
| Selected tokens | 90,850 | 90,850 |
| Full typed render and ordered omission digest | `10a31bb5c3e203a2e4833fbcdf5260d824b8d7bd4c79a40fbcffd110a2ca4481` | Same |

This reproduces an over-eight-second CPU stall with the user's actual legal working state,
below its 512-entry limit. The old and new complete typed outputs match exactly. The allocation
headroom retries amplify the original repeated scans even when a single small graph selection
looks inexpensive. The measurement is a local replay, not a general performance guarantee.

## Systematic public-interface checks

The independent consumer is [context_load.rs](reproducers/tui-reconnect/context_load.rs).
The same consumer was compiled against a private baseline crate with only the two changed
context files restored from `57440fc16475237dc2bf3609996f4991e1186840`, and against the final
actual library. It executes the real public API, not unit-test hooks or copied selector logic.

Both libraries passed the same 62 generated semantic scenarios. Inputs vary independent nodes,
forward/backward required chains, optional chains, shared diamonds, hidden dependencies and
token budgets. Oracles independently specify exact selected IDs, render order, admission reasons,
complete closures, atomic omissions, typed errors and accounting. There is no random seed.

| Separate legal graph load | Baseline selection | Fixed selection |
| --- | ---: | ---: |
| 128 isolated nodes | 1.085 ms | 0.065 ms |
| 512 isolated nodes | 56.562 ms | 0.392 ms |
| 1,024 isolated nodes | 338.076 ms | 1.027 ms |
| 2,048 isolated nodes | 2,516.327 ms | 3.075 ms |
| 4,096 isolated nodes | 19,446.031 ms | 10.649 ms |
| 512 optional-chain nodes | 4,520.212 ms | 11.115 ms |

The 4,096-node input is a public ContextGraph stress case, not a claim that the user's working
state contained that many entries. Each stress invocation had a 35-second deadline and ran
one consumer process. The build used two Cargo jobs. No unit, formal or repository verification
suite was run.

## Real daemon and terminal replay

Build and launch:

```sh
cargo build --locked --release --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
uv run --offline --with pyte python3 docs/testing/reproducers/tui-reconnect/replay.py --binary target/release/peritus --output /home/USER/pr-rootfix --with-retry --attach-checks 2
```

The real CLI/TUI creates isolated XDG state and uses the actual companion daemon. It first
exercises graceful shutdown, SIGKILL, offline edits and automatic recovery, then submits a short
read-only Plan through the real ChatGPT account/vendor CLI (`gpt-5.6-sol`). Two additional
ordinary terminal windows open that same active run through the normal launcher.

PASS: both fresh windows reached the run on generation 1, with no failed attempt; observed
attachment times were 0.158 and 0.172 seconds. The original window then deliberately stopped and
retried that same run. After another actual daemon restart, run identity and public-input count
remained unchanged, with two total model requests and no automatically reissued Retry.
`notes.md` stayed unchanged; all fixture clients exited successfully and owned daemons stopped.

A separate independent fresh daemon/TUI replay passed graceful and abrupt loss, exact Unicode
offline draft editing, automatic recovery and a subsequent real `/doctor` response. It observed
private log mode `0600`, no draft text in the log, unchanged fixture data and no remaining owned
fixture processes.

Final artifacts exercised:

| Binary | SHA-256 |
| --- | --- |
| `peritus` | `d67fb90f1193246e5538748c930750c5bd5ffb0cef734769d63712e03f9a46b8` |
| `peritusd` | `d3ae3c292e9c9f8f4bb2f02b6565ff7577990cf7693e864effefb97036b3fee2` |
| `peritus-tui` | `c83101aa24bc25a2a2d517136a4c48e01f67f2dce161500c850cab9289dad146` |

Raw local evidence lives under `/home/doll/Peritus-Handshake-2026-10-15`, including the profile,
traced status request, frozen baseline consumers, exact copied snapshot, baseline/fixed snapshot
results, generated-case results and `independent-context-report.json`. Actual terminal results:
`/home/doll/prn-rootfix/result.json` and `/home/doll/prn-independent5/result.json`.

## Limits and activation

The original live client log proves two handshake timeouts followed by successful connection;
profiling plus exact snapshot replay establishes the expensive context path. The isolated
full-program replay establishes normal first-attempt attachment while the rebuilt daemon runs
a real task. It does not inject that entire copied long-running conversation into a second
live inference task or claim every possible OS, storage, scheduling or transport stall is gone.
Windows/macOS and a multi-hour post-update observation are outside this replay.

Replacing an installed binary does not change a daemon already loaded in memory. The live
daemon and browser run were left untouched during diagnosis and testing; activation requires
a graceful restart and resumption of the same saved run.

## Live activation and interrupted-start handoff

After the user closed the terminal client, the supported `shutdown --wait` command completed
all six phases cleanly with zero remaining work. The original run was retained as
RecoveryRequired, with 27 public inputs and 7,995 model requests. The installed daemon was
atomically replaced with the tested `d3ae3c…` artifact; its prior executable was retained.

The first launcher was interrupted while its detached daemon was still performing domain
recovery. The daemon survived and finished in 16.475 seconds, but its launcher-owned
`applied-configuration` receipt still named the old executable. Four additional user launch
attempts during that recovery tried to start competing daemons and failed the singleton lock.
This is an additional confirmed interrupted-start handoff defect; the selector repair did not
cause loss of the run or its state. The source-level interrupted-start defect is not repaired
by this campaign's transport/selector changes.

Operational recovery checked the actual running process's configuration arguments and the
SHA-256 of `/proc/PID/exe`, observed authenticated ready status, then republished the matching
startup receipt under the normal launcher lock. No unverified configuration or executable
was adopted. Exactly one supported `runs retry --run …` resumed the original identity.

PASS: the user's original terminal subsequently reopened the saved run on generation 1,
with no connection failure. The daemon advanced from the retained queue to Writing and
started actual provider request 7,996. Both the selected run and public-input count remained
unchanged. The live daemon is PID 70,228, running the tested executable; the user client at
readback was PID 71,670. Private activation metadata and terminal readback are retained as
`activation-complete.json` and `live-restored-terminal.txt` in the external evidence folder.
No further restart was performed after successful reattachment.
