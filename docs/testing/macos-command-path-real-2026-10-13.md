# macOS installer command-path failure

The reported release 0.0.7 failure is consistent with the launcher looking next to the public CLI symlink rather than next to the installed package:

```text
packaged daemon executable is unavailable: ~/.local/bin/peritusd
```

The published package **contains the daemon**. Its native installer copies both binaries to `~/Library/Application Support/Peritus/bin`, then creates only the CLI shortcut `~/.local/bin/peritus`. Before this correction, `SiblingBinaries::discover` used `current_exe().parent()` directly. [Rust documents platform-dependent symlink paths](https://doc.rust-lang.org/std/env/fn.current_exe.html), and [Apple documents that `_NSGetExecutablePath` may report a symbolic link](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/dyld.3.html). The user's screenshot/error supplies the native macOS observation; no native Mac was available for an independent reproduction here.

## Fix

`crates/app/peritus-launcher/src/daemon.rs` resolves the application executable itself with `fs::canonicalize` before sibling lookup. Resolving only its parent would leave the shortcut in the wrong directory. `discover` uses the public `from_application` package-location constructor, and explicit-daemon construction also retains a canonical application path. Regular-file and exact daemon version-output checks remain in place. Missing, broken or mismatched package files still fail with an actionable path.

No installer component was removed, no PATH fallback or unrelated daemon is accepted, and no unit tests or verification gates were added or run. The keyboard/mode changes already in the working tree were preserved.

## Actual published-package evidence

Downloaded the [v0.0.7 Apple-silicon archive](https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/download/v0.0.7/peritus-macos-aarch64.tar.gz) and its sidecar. Archive SHA-256: `21d3abf635a76a0d1ce7d1198cd1252d01241778b3509f99ae96f50804a66e4c`, matching the published checksum. All eight internal `SHA256SUMS` entries were independently read and checked. `bin/peritusd` is a real Mach-O 64-bit ARM64 executable. The published native installer matches the checked-out macOS installer and explicitly publishes `bin/peritusd`. The [release bootstrap](https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/download/v0.0.7/install.sh) invokes that installer.

## Observable contract and real replay

Expectations were defined before execution: an application alias must use the daemon beside its actual package file; an adjacent command-directory binary must not override that sibling; invalid package paths and wrong-role or non-executable daemons must reject; recovery after a rejected package must succeed. Installed startup must connect to a real daemon, and the repair must replace only the public shortcut while preserving the actual CLI bytes and forwarding its arguments.

Build:

```sh
cargo build --offline --release -p peritus-cli -j 2
```

A minimal real consumer linked the current production `peritus-launcher` library. Each successful lookup executes the actual compiled daemon's `--version` probe. The independent fixture oracle names the known package paths; it does not copy the production resolution algorithm. Twenty scenarios passed: direct invocation; absolute/relative aliases; spaces, accented/CJK and literal shell characters; two/three-link chains; directory aliases; misleading adjacent binaries; dangling/cyclic links; directory/missing applications; missing, wrong-role, non-executable and directory daemons; and successful recovery. An independent agent reopened four aliases and matched the observed executable paths and artifact hashes.

The wrong-role case uses the real CLI as the would-be daemon. It proves rejection of the wrong exact version-output identity, not a same-role different-version daemon case. Native Windows behavior and macOS's acquisition of `current_exe` were not executed by this portable filesystem consumer.

The published bootstrap was also run through actual curl/checksum/extraction/native-install operations, followed by an actual repeated upgrade, against a documented local mirror containing the current real Linux executables. Bubblewrap bound disposable home directories over the existing HOME path only inside its mount/PID namespaces; the HOME variable and live user installation were not changed. Automatic dependency installation was disabled, existing real system dependencies were used, and no providers or inference tasks were started.

The unchanged macOS native install adapter was separately executed on Linux with real Linux CLI/daemon/TUI consumers and the actual published Mac helper bytes, using a truthful Linux fixture manifest. This exercises the adapter's file copying and shortcut layout, **not native Mach-O execution**. It established that the daemon resides in the package `bin` while the public command is a symlink. The exact proposed repair command then atomically replaced that symlink with a shell launcher. The packaged CLI's SHA remained unchanged; CLI/daemon version probes passed; forwarded `open` arguments reached normal setup; and the real TUI connected to an actual fixture daemon. Both installation paths exited the client normally and shut down their daemon Clean with zero remaining work. Their isolated PID namespaces ended, leaving no campaign-owned daemon.

| Scenario | Observed result |
| --- | --- |
| Actual public Mac archive and internal file readback | Daemon present; checksums match; Mach-O ARM64 identity observed |
| Production package-discovery consumer | 20/20 PASS, with four independent alias replays |
| Published bootstrap, current Linux payload, fresh install and repeated upgrade | PASS |
| Native Mac installer filesystem adapter, Linux payload | PASS within the explicitly stated cross-host scope |
| Exact repair wrapper: replacement, argument forwarding, version probes and startup | PASS with real Linux consumers in the Mac file layout |
| Real installed TUI → actual fixture daemon → orderly cleanup | PASS on both Linux install and Mac-layout repair paths |
| Native Mac CLI/daemon execution and physical macOS user setup | Not available; user-provided failure is retained, independent native replay remains open |

## Reproduction artifacts

Raw files are under `/tmp/peritus-installer-real-20261013/`: original published archive/bootstrap, actual extracted package, `discovery/results.json`, installer logs, terminal captures, CLI/daemon version outputs, and Clean shutdown receipts. The real consumers and replay drivers are under `docs/testing/reproducers/macos-command-path/`. They create only disposable package copies and invoke actual program interfaces. They do not substitute application or daemon responses. The installer replay requires bubblewrap and existing Linux runtime dependencies; the consumer requires an ordinary current release build.

The fix and installation troubleshooting note are in the develop working tree. No published release assets, local user installation, credentials, live daemon/client or active browser-building run were changed.
