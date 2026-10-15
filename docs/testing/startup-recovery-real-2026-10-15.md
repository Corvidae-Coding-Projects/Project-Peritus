# Daemon startup and folder recovery: real-interface campaign

## Contract and planned scenarios

The reported `peritus open PATH --run ID` must reconnect to the existing in-place workspace and saved run after a reboot. Startup must preserve all histories and integrity checks while reducing unnecessary work. No unit tests, mocks, formal verification, Clippy or repository verification suites are part of this campaign.

Before final replay, the expected outcomes are:

| Scenario | Expected result |
| --- | --- |
| Existing large archive | The real daemon opens a disposable copy of the existing 8,500-request archive, preserves every state value and original run ID, and serves actual CLI readback. |
| Transactional layout upgrade | Schema 3 advances to 4 without changing any state bytes, digest, revision, producing position, event or command. Restart does not repeat the copy. |
| Repeated startup | Recovery after migration has fewer unnecessary reads; no archive/integrity check is removed. |
| Startup longer than 30 seconds | The actual launcher waits, shows recovery progress, and attaches when the real daemon becomes ready. |
| Existing folder becomes a Git repo | Reopening keeps its direct-folder workspace ID, trust and conversations. |
| Legacy identity | Matching legacy trust upgrades automatically. Changed identity requests renewed trust, preserving lineage even when trust is declined. |
| Directory replacement | Replacing a remembered directory cannot silently inherit write authority. |
| Live repair | Matching installed CLI/daemon/TUI binaries open the user's exact saved run in its original workspace, without retrying the cancelled coding task. |

## Initial evidence

A directly started installed daemon completed recovery in 16.58 seconds after prior reads warmed the filesystem cache. A disposable reflink copy of the actual workbench archive required 34.90 seconds and 48.44 GB of logical reads before its real IPC endpoint became ready. The original launcher kills its child at 30 seconds. Actual `runs show` succeeded when the baseline daemon was allowed to finish.

The state table used `WITHOUT ROWID`, placing large value payloads inside the primary-key B-tree. A read-only SQL measurement of 800 original archive request/manifest pairs retained SHA-256 checks and read 4.296 GB. The same lookups on an experimentally converted disposable table read 332 MB. This establishes the storage-layout cause; final product migration and daemon measurements remain to be recorded below.

Evidence is private under `/home/doll/Peritus-Cold-Startup-2026-10-15`; no credentials or user conversation contents are copied into this report.

## Product replay results

Built optimized CLI, daemon, standalone TUI and WebUI gateway with `cargo build --locked --release --jobs 2 -p peritus-cli -p peritus-daemon -p peritus-tui -p peritus-web`. No unit/formal/lint/qualification suites were invoked.

The actual product migrated the copied 4.4 GiB control store from schema 3 to 4 and became ready in 43.42 seconds, including the one-time transactional table copy. Subsequent actual daemon startups took 16.49 and 14.83 seconds and made 3.633 GB of logical reads each, versus the original 34.90-second, 48.442 GB replay. These timings are filesystem/cache/load observations, not a controlled benchmark; the approximately 92.5% reduction in logical reads is the stable improvement. No digest, chain, receipt or archive validation was removed.

Independent read-only comparison streamed exact rows across all 19 control-database tables. Every value byte and field matched, except the intended schema-version change. Preserved counts include 28,917 current states, 8,500 request archives, 8,500 manifests, 11,827 events, 11,827 commands, 40,730 historical records and 1,654,539 history nodes. A separate actual CLI process read the original run successfully in 13 ms and confirmed its exact run/workspace IDs and Cancelled state. The comparison took 18.82 seconds and peaked near 595 MiB RSS in the Python checker; that is checker overhead, not daemon memory.

The folder's original digest was independently reproduced from the same canonical path and inode with device number 37; the current mount reports 52. Thus the identity mismatch really was caused by device renumbering. Version 2 uses inode plus directory birth time on Unix filesystems that provide it. Unsupported birth-time filesystems retain the prior identity and explicit repair path. Matching version-1 trust can migrate without a prompt; an unprovable identity change requires renewed trust and retains all nominal workspace/conversation IDs.

The real OS timing replay delayed `listen` for 35 seconds. The original launcher printed its 30-second timeout and killed its child. The updated launcher displayed recovery progress and reached the actual TUI in 35.64 seconds. Three subsequent real launches initialized Git, replaced the directory while retaining the old one, declined trust, and used the actual `workspaces`/`t1` menu to restore trust. The workspace ID stayed unchanged; declined replacement stayed restricted. The replay driver's first settings attempt used pipes, which the CLI correctly rejects for interactive settings; the corrected driver uses a PTY and passed.

The timing replay also found a separate real recovery defect: killing the old daemon after socket bind but before chmod left an owned non-0600 socket. Startup rejected that unfinished socket forever. Recovery now permits removing an owned stale socket of any mode only after instance-lock acquisition inside an owner-only parent. Regular files, symlinks and other owners remain rejected; live listeners still require mode 0600 and authenticated OS peers. Final interruption and rejection replays are recorded below.

Final updated binaries successfully reopened the actual non-0600 socket left by the old-version timeout and migrated the old fixture's legacy profile without a prompt in 0.26 seconds. The original workspace ID and trust were retained. Actual daemon starts with a regular file and then a symlink at that socket path both rejected the object with exit 1 and left its bytes/target unchanged. A second full lifecycle sequence used a Unicode-and-space XDG path and passed all four launches plus the terminal trust menu. A real public `FolderInspection` consumer independently opened the new folder identity and returned all original document bytes exactly.

Exploration covers two complete folder lifecycle sequences, a delayed startup replay before and after the fix, fresh legacy/crash recovery, two distinct invalid endpoint occupants, two post-migration brownfield restarts, exact public run readback, and a full independent database byte comparison. Filesystem, timing and Linux resource observations are bounded to these scenarios. No reboot, native macOS or Windows run was performed. Birth-time persistence across reboot follows filesystem metadata; the original device-renumbering cause was confirmed by reproducing the previous identity with device 37 and the same current inode.

Before personal installation, the original immutable product state, configurations, control database/continuations and daemon database were copied into a private backup. The lost original browser profile was restored through the public bootstrap API, keeping every original nominal ID and observing the current exact folder identity. The newer managed copy created during the failed user launch was retained. Four optimized binaries were atomically installed, with pre-install binaries retained separately. SHA-256 readback matched every built/installed executable.

## Installed live result

The exact reported command was replayed with the installed binary:

```sh
peritus open /home/doll/browser --run 81f9302c94ce667a2fff398d434f6192
```

PASS: the installed daemon became ready in 26.70 seconds, including its first control-store migration; the terminal connected at 26.88 seconds. A fresh client reused that daemon in 0.15 seconds. Waiting for actual conversation loading displayed the original `workspace_read` history and `Cancelled — in-place effects retained, not verified complete`, with received/incorporated input counts 26/26. The first short connection-only observation showed the landing screen while history was still loading; the extended readback establishes actual run opening. The saved run JSON is exactly unchanged from the private pre-install backup. No user message, retry or provider inference was submitted.

The personal daemon is running the installed `/home/doll/.local/bin/peritusd`. The temporary replay clients and disposable daemons were shut down. The restored browser workspace is the original `a9f0183050b6c78adcf62641de464744`; the failed launch's separate managed copy remains on disk. Build provenance, private readbacks and backups are retained in the external evidence folder. The saved run remains Cancelled until the user deliberately continues it.
