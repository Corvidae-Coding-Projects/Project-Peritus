# OpenCode runtime on WSL — 2026-10-20

## Confirmed defect

A user in a WSL distribution selected OpenCode Go through `peritus providers`, supplied an
entitled key, and received:

```
Credential captured. Saving it to the operating-system credential store…
Discovering available models from this provider…
model discovery failed: PERITUS-PROVIDER-CORE-009 model_catalog: OpenCode did not return a
complete service model catalog; install a supported official CLI and retry
```

The same key and the same Windows OpenCode installation work from native Windows on the same
machine, and the WSL `opencode` command works there too. The failure was therefore specific to
which executable Peritus pinned.

WSL appends the Windows `PATH` to the distribution `PATH`, so discovery reached the Windows npm
installation at `/mnt/c/Users/Admin/AppData/Roaming/npm/opencode`. That file is an npm shell shim
that executes the vendor's Windows `opencode.exe` through WSL interop. It is a regular,
executable file on a Windows-drive mount, and `--version` prints exactly `1.19.0`, so it passed
both the file pin and the version probe that gate a supported runtime. The transport then ran
the metadata command through the same interop path, where the vendor program failed:

```
$ /mnt/c/Users/Admin/AppData/Roaming/npm/opencode models opencode-go --verbose
Error: Unexpected error

Database is not empty and has no session table
exit 1
```

That failure is independent of the isolated configuration Peritus supplies (private XDG roots,
`OPENCODE_TEST_HOME`, `PWD`, and a private `auth.json`) and of the working directory: it also
occurs with the ambient Windows environment and from a Windows filesystem directory, while
`--version` and `models --help` keep succeeding. The Windows program cannot operate on the
POSIX paths this transport owns, so the fail-closed catalog parse correctly refused to invent a
model list — but the reported cause pointed the user at the wrong remedy.

Two other candidates were present and were correctly skipped: the vendor's own Linux
installation reported `opencode v2.0.32` and the user's Node installation reported `1.18.31`,
both outside the pinned 1.19.0 contract. The message that followed ("install a supported
official CLI and retry") therefore also misled: the Windows installation *was* a supported
official CLI for Windows.

## Final behavior and independent expectations

- Discovery pins only a native host executable. On a Unix host a Windows program — either a
  portable-executable image or anything reached through a WSL distribution's Windows-drive
  mount — is not a supported runtime, regardless of the version it reports.
- A foreign candidate is skipped, not fatal. Discovery continues through the remaining `PATH`
  and per-user installation directories, so a native OpenCode 1.19.0 is still found when a
  Windows entry precedes it.
- When no native runtime is available the user sees the actionable installation path: the
  provider-settings flow offers to install the official CLI, and declining reports
  `Install the official OpenCode CLI, then retry provider setup.` instead of a catalog failure.
- An explicitly pinned foreign executable is rejected with its own configuration diagnostic
  rather than a version or catalog error.
- Nothing else changed: native OpenCode 1.19.0 discovery, advertised-model metadata, model
  selection, and the isolated vendor invocation are unchanged.

These expectations come from the transport's stated contract — the vendor CLI runs as a native
host process with a private POSIX configuration, state, and temporary root — and from the
reported failure itself. The reproducer does not reuse the production predicate: it classifies
the pinned path with an independent oracle (portable-executable header, Windows-drive mount)
and compares that classification with the production decision.

## Observed coverage

| Journey | Independent expectation | Observed result |
| --- | --- | --- |
| Pre-fix provider settings, WSL with interop | Native runtime pinned, no catalog failure | Failed: pinned the Windows npm shim, then `PERITUS-PROVIDER-CORE-009 model_catalog` (installed 0.1.4 and the pre-change checkout build) |
| Pre-change retained consumer | Consumer must report a foreign pinned runtime | `runtime /mnt/c/Users/Admin/AppData/Roaming/npm/opencode`, `nativity foreign (WSL Windows-drive mount)`, exit 1 |
| Post-change provider settings, no native runtime | Actionable installation path, never a catalog error | Passed: installation prompt appeared; declining reported `Install the official OpenCode CLI, then retry provider setup.` |
| Post-change retained consumer, no native runtime | No foreign runtime pinned | Passed: `runtime none`, exit 0 |
| Post-change, official Linux 1.19.0 on `PATH` after the Windows entry | Skip the foreign candidate, pin the native one, list models | Passed: `runtime /tmp/oc119/bin/opencode`, `nativity native`, `catalog 31 advertised models`, exit 0 |
| Post-change provider settings, native 1.19.0 present | No installation prompt; advertised model list | Passed: 31 OpenCode Go models listed through the real terminal flow |
| Native release asset integrity | Published digest matches | Passed: `opencode-linux-x64.tar.gz` sha256 `7bb9e61a…076aec`, ELF x86-64, `--version` = `1.19.0` |

## Evidence and replay

The retained consumer and its driver live in
[reproducers/opencode-wsl-interop](reproducers/opencode-wsl-interop/README.md); the exact pre-fix
path probes, vendor failure, and provider-settings transcripts are in
[observed-baseline.txt](reproducers/opencode-wsl-interop/observed-baseline.txt).

```bash
# Pre-change detection and post-change confirmation of the same real program.
python3 docs/testing/reproducers/opencode-wsl-interop/replay.py --output /absolute/new-owned-dir

# The supported path, with a native 1.19.0 appended after the Windows entry.
PATH="$PATH:/path/to/native-1.19.0/bin" \
  python3 docs/testing/reproducers/opencode-wsl-interop/replay.py --output /absolute/new-owned-dir

# The user-visible flow.
cargo build --locked --jobs 2 -p peritus-cli
./target/debug/peritus providers
```

Run the consumer from a WSL distribution with interop enabled and a Windows OpenCode
installation on `PATH`. A restricted sandbox that blocks WSL interop hides the defect, because
the foreign candidate then fails the version probe for an unrelated reason.

## Boundaries

- No inference, entitled key, or stored credential was used. Discovery is metadata-only, and
  the placeholder key in the consumer is never stored. Model access, Go entitlement, and
  generation were not re-exercised here; they are covered by the
  [OpenCode runtime campaign](opencode-runtime-real-testing-2026-10-13.md).
- OpenCode Zen was not exercised; the Go service was used because it reproduced the report.
- The Windows-drive mount detection is scoped to WSL kernels. A Windows program reached on a
  non-WSL Unix host is still rejected by the portable-executable check.
- macOS and native Windows discovery paths were not exercised on those systems.
- The user's own Linux `opencode` (v2.0.32) remains outside the pinned 1.19.0 contract; this
  change does not add support for that version.
