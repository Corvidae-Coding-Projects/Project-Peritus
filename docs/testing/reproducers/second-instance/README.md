# Multiple terminal windows

Operate the actual Peritus CLI/TUI and daemon with new, isolated XDG configuration/state/cache directories. The driver uses PTYs and `pyte` to observe the terminal; it supplies no substitute daemon or provider. Keep the output outside live workspaces. The output directory must not already exist.

Prerequisites: Linux, `ss`, Python, `pyte` 0.8.2, and a built `peritus` with its matching `peritusd` sibling. The optional Git case also needs Git. No unit suites or repository verification gates are invoked.

```sh
uv run --with pyte==0.8.2 --no-project python \
  docs/testing/reproducers/second-instance/replay.py \
  --binary /absolute/path/to/bin/peritus \
  --output /absolute/path/to/new-disposable-fixture
```

The independent contract is: both clients become ready on one daemon; adding a window preserves the first connection; each selected workspace has the permission the operator selected; reconnect and closing the other window preserve access. The driver records the daemon PID and process start token, actual terminal screens, executable digest, and machine-readable results. A violation exits nonzero after owned cleanup. `--expect-bug` retains a baseline failure without treating it as an unexpected driver error.

Distinct input/lifecycle cases:

| Option | Journey |
| --- | --- |
| Default | Two writable plain folders with spaces and Unicode names. |
| `--bare` | Launch bare `peritus` from each folder, without the explicit `open` command. |
| `--git` | Two actual Git repositories and their managed workspaces. |
| `--second-read-only` | A read-only second window beside a writable first window. |
| `--first-read-only` | A writable second window beside a read-only first window. |
| `--same-workspace` | Two independent clients selecting the same directory. |
| `--unavailable-inactive` | Remember and close a spare folder, make it unavailable, open another window, then restore and reopen the spare on the same daemon. |
| `--reject-updates` | Runtime-policy changes, unknown configuration fields, oversized input, and symlinked update files; status stays available and admitted metadata does not advance. |
| `--ack-recovery` | Force an acknowledgment publication failure in owned state, confirm the first window survives, restore storage, and retry the second window. |
| `--upgrade-from /path/to/old/bin/peritus --first-read-only` | Prepare a legacy read-only installation, shut down only its disposable daemon, then open it with the new binary. Original immutable configurations must remain byte-identical. |
| `--upgrade-from /path/to/old/bin/peritus --legacy-timeout` | Prepare an old installation with its retired provider timeout field, then open both folders using the new binary without rewriting the old configuration. |
| `--active-task` | Use an authenticated real Codex account. Observe the first planning request's provider turn before opening the second folder, then request a second task there. Confirm both exact workspace canaries and unchanged fixture files. Record whether provider turns overlap. This consumes provider usage. |

The active task inspects a short script; it does not execute the script or invoke the coding pipeline. It establishes concurrent operation during a real model request, not hours of execution or a full coding/reviewer/fixer campaign. macOS and Windows native execution are outside this Linux replay.

Use [AGENTS.md](AGENTS.md) for source ownership and [the report](../../second-instance-real-2026-10-15.md) for observed results and limitations.
