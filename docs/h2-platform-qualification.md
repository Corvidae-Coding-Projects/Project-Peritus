# H2 platform qualification and packaging

H2 is the release-layout and packaged-host acceptance boundary for Peritus. It qualifies the same
`peritusd`, `peritus`, `peritus-tui`, C2 process contract, and C3 native backend already defined by
G0-G2 and C2/C3 after those artifacts have crossed a real package installation boundary. H2 does
not grant runtime authority, reinterpret a sandbox plan, or manufacture configuration. A target is
ready only after every required scenario has passed on its own fresh subject with complete cleanup.

The implementation lives in `peritus-platform-qualification`. The crate contains deterministic
contracts, manifest/checksum generation, evidence bounds, fresh-subject orchestration, and verdict
reduction. Operating-system adapters perform effects through `FreshSubjectFactory` and
`QualificationSubject`; native package scripts remain mechanical reviewed assets in `packaging/`.

## Shipped processes and invocation

Every target package contains the three application processes and its C3 helper:

| Target | Application binaries | Native helper |
| --- | --- | --- |
| Linux | `peritusd`, `peritus`, `peritus-tui` | `peritus-linux-sandbox-helper` |
| macOS | `peritusd`, `peritus`, `peritus-tui` | `peritus-macos-sandbox-helper` |
| Windows | `peritusd.exe`, `peritus.exe`, `peritus-tui.exe` | `peritus-windows-sandbox-helper.exe` |

The helper is an inert C3 implementation artifact until the C2 gateway consumes exact authority
and invokes the admitted backend. It is never a general process launcher.

G0 has one production invocation:

```text
peritusd serve --config <absolute-path-to-peritus.toml>
```

systemd, launchd, and Task Scheduler supervise that foreground process directly. No package uses a
shell wrapper, hidden daemonization flag, remote transport, or service-specific daemon command.
The G0-only `qualify-pty` and outbox qualification commands remain test administration entry points
and are not used for normal service startup.

The packaged `peritus` product entry discovers these paths and its stable endpoint automatically:

```text
peritus
```

The G1 and G2 operator surfaces continue to accept an explicit endpoint for diagnostics and
automation:

```text
peritus --endpoint <unix-socket-or-windows-pipe> status
peritus-tui --endpoint <unix-socket-or-windows-pipe>
```

## Per-user layouts and ownership

Peritus is packaged per user because G0 authenticates the operating-system peer against the owner
of the protected state root. `ReleaseLayout::production` materializes these exact layouts for a
concrete home directory.

### Linux

| Ownership | Path |
| --- | --- |
| Package | `~/.local/bin/{peritusd,peritus,peritus-tui}` |
| Package | `~/.local/libexec/peritus/peritus-linux-sandbox-helper` |
| Package | `~/.config/systemd/user/peritus.service` |
| Operator | `~/.config/peritus/peritus.toml` |
| Runtime | `~/.local/state/peritus` |
| Runtime | `~/.local/state/peritus/log` |

Directories containing protected data are mode `0700`; strict config, service definition, state
files, and Unix endpoint are mode `0600`; installed executables are mode `0755`. systemd captures
stdout/stderr in the user journal. The unit uses `UMask=0077`, a five-second failure restart delay,
a five-attempt/300-second start limit, a 40-second shutdown bound, and `KillMode=mixed` so G0 gets
the first orderly signal before the supervisor's terminal cleanup bound.

### macOS

| Ownership | Path |
| --- | --- |
| Package | `~/Library/Application Support/Peritus/bin/{peritusd,peritus,peritus-tui}` |
| Package | `~/Library/Application Support/Peritus/libexec/peritus-macos-sandbox-helper` |
| Package | `~/Library/LaunchAgents/com.corvidae.peritus.plist` |
| Operator | `~/Library/Application Support/Peritus/config/peritus.toml` |
| Runtime | `~/Library/Application Support/Peritus/state` |
| Runtime | `~/Library/Logs/Peritus` |

The rendered LaunchAgent uses direct `ProgramArguments`, `RunAtLoad`, failure-only `KeepAlive`, a
five-second throttle, umask `0077`, and separate owner-private stdout/stderr files. launchd has no
finite retry-count analogue; H2 observes the native throttled failure restart instead of claiming
the systemd/Task Scheduler attempt ceiling exists on macOS.

### Windows

| Ownership | Path |
| --- | --- |
| Package | `%LOCALAPPDATA%\Programs\Peritus\bin\{peritusd,peritus,peritus-tui}.exe` |
| Package | `%LOCALAPPDATA%\Programs\Peritus\libexec\peritus-windows-sandbox-helper.exe` |
| Package | `%LOCALAPPDATA%\Peritus\supervisor\Peritus.Task.xml` |
| Operator | `%LOCALAPPDATA%\Peritus\config\peritus.toml` |
| Runtime | `%LOCALAPPDATA%\Peritus\state` |
| Runtime | `%LOCALAPPDATA%\Peritus\logs` |

The installer removes inherited broad access and grants the current user full control over the
package and protected data directories. The Task Scheduler definition uses an interactive-token,
least-privilege principal, the exact current user logon trigger, direct executable plus arguments,
five retries at five-second intervals, and no execution-time ceiling. Supervisor lifecycle is
retained in the Task Scheduler Operational event log. Peritus telemetry remains disabled or the
strict bounded local-file spool selected by G0 configuration; packaging does not invent a second
application log sink.

## Configuration and endpoint discovery

Legacy supervisor-first installation may retain an existing regular `peritus.toml`. The ordinary
product path does not require one: the no-argument launcher creates protected platform-local roots,
generates stable non-secret store and actor identities, publishes the canonical public approval
registry, renders strict G0 configuration, and starts or reuses the packaged daemon. Later G4
onboarding phases add provider and trusted-workspace declarations through the same typed product
state instead of asking the user to edit TOML. The configured `state_root` and all six component
roots remain absolute, normalized, nonoverlapping children as required by G0.

G0 derives a stable non-secret endpoint name from the exact 16-byte C0 store identity:

```text
sha256("peritus/daemon-endpoint/v1\0" || store_id)[0..16]
endpoint-name = "peritus-" || lowercase-hex(digest-prefix)
```

On Linux and macOS the address is `<state_root>/<endpoint-name>.sock`, owned by the state-root user
at mode `0600`. On Windows it is `\\.\pipe\<endpoint-name>`, created with an owner-restricted
security descriptor. `state_root/daemon.instance` publishes the live endpoint name, PID, and
process birth token while the instance lock is held. The record disappears on orderly teardown;
it is discovery evidence, not authority. No platform permits a TCP or remote listener.

## Deterministic release manifests

`PackageManifest` validates and renders strict schema-one TOML. It binds:

- release version, platform, and architecture;
- the complete `ReleaseLayout` digest;
- exactly one G0 daemon, G1 CLI, G2 TUI, C3 helper, and native supervisor definition;
- installer, upgrader, and uninstaller scripts;
- canonical package-relative path, byte length, SHA-256, and executable status for every artifact.

Artifacts are sorted by package-relative path. Duplicate paths, repeated singleton roles, missing
roles, empty executables, unbounded manifests, noncanonical paths, unknown fields, and
noncanonical TOML bytes are rejected. The same model emits deterministic `SHA256SUMS`. Native
installers verify all staged artifact hashes before stopping or replacing an installed process.
The release system remains responsible for signing or otherwise authenticating the manifest
digest before a subject trusts it.

## Install, upgrade, rollback, and uninstall

`LifecyclePlan` separates package, supervisor, and runtime effects.

Fresh install verifies the target and staged manifest, requires operator configuration, publishes
package files by temporary sibling plus rename, applies exact modes/ACLs, registers autostart,
starts G0, then requires authenticated G1 readiness on the derived endpoint. Failure compensation
stops G0, removes supervisor registration, and removes only package-owned entries.

Upgrade stops and observes the old daemon, snapshots only package-owned files, publishes and
protects the new package, replaces the supervisor definition, starts G0, and waits for
authenticated readiness. It then verifies protected config, state, and logs remain outside the
package mutation set. If any forward step fails, rollback stops the new daemon, restores the prior
package files and supervisor definition, restarts the prior package, and requires readiness before
reporting restoration. Diagnostic logs are retained across the attempt.

Ordinary uninstall stops G0, unregisters autostart, removes the supervisor definition, helper, and
application binaries, and verifies that configuration, durable state, logs, and credential stores
remain. There is no purge path in the reviewed package scripts. Removing durable state requires a
separate explicit operator action outside H2's ordinary uninstall authority.

## Platform and sandbox prerequisites

H2 does not reinterpret C3 support:

- Linux production subjects are x86-64 or AArch64, kernel 6.6 or newer, with functional
  user/mount/PID/IPC/UTS/network namespaces, reviewed bubblewrap, Landlock ABI 3+, seccomp-BPF,
  plan-required delegated cgroup v2 controllers, PTY support when requested, Secret Service when
  credentials are required, and the systemd user manager.
- macOS production subjects are macOS 15 or newer on Apple Silicon or x86-64, with the packaged
  helper, functional Seatbelt, process-group and requested PTY ownership, required rlimits,
  Keychain when credentials are required, managed-proxy reachability when egress is requested,
  and a per-user launchd domain.
- Windows production subjects are Windows 11 24H2 or Windows Server 2025 build 26100+ on x86-64,
  with restricted tokens, low-integrity/AppContainer support, kill-on-close Job Objects, exact
  handle-list inheritance, reversible ACL controls, ConPTY when requested, Credential Manager
  when credentials are required, Task Scheduler, and BFE/WFP management when managed egress is
  requested. The retained Windows AArch64 build path is not an H2 production qualification claim.

The native `SandboxObservation` binds the package helper digest, native probe digest, each of the
seven C2 capability domains plus recovery, literal argv, a directly executed restricted process,
absence of raw fallback, and complete release. Required unsupported controls produce not-ready;
they are never converted to a weaker backend claim.

## Process equivalence

H2 compares installed invocations with release-control invocations after only the six declared
platform normalizations: endpoint address shape, supervisor, log ownership, PTY versus ConPTY,
signal versus Windows exception representation, and native sandbox mechanism. `ProcessObservation`
then requires exact executable digest, structured arguments, terminal classification,
stdout/stderr digests, protocol observation digest, and complete tree cleanup. No behavior-bearing
field is allowed to differ after normalization.

This prevents an installer wrapper, alternate binary, shell parsing layer, missing helper, output
truncation, or cleanup loss from passing because an application-level happy path looked similar.

## Fresh-subject scenarios and evidence

The closed catalog contains 18 required scenarios:

1. artifact integrity;
2. release layout;
3. protected roots;
4. service autostart;
5. service restart;
6. local transport;
7. peer authentication;
8. CLI status;
9. TUI negotiation, rendering, shutdown, and terminal restoration;
10. process equivalence;
11. pipe separation;
12. PTY/ConPTY ownership;
13. cancellation and complete tree reap;
14. native sandbox denial without fallback;
15. admitted native sandbox execution and complete release;
16. upgrade preservation;
17. failed-upgrade rollback; and
18. uninstall preservation.

`FreshSubjectRunner` asks the adapter for a different never-used subject identity for each
scenario. It closes the subject whether execution passes or errors, binds cleanup to the same
identity, rejects subject reuse or scenario substitution, and accepts a complete run only in the
canonical catalog order.

Scenario evidence is non-secret, canonical, and bounded to 64 labelled entries and 256 KiB.
Individual text is bounded to 8 KiB and rejects display controls; raw output is retained by size
and SHA-256 rather than copied without limit. `QualificationRun` requires cleanup evidence for
every subject. `QualificationReport` returns `Ready` only when all 18 outcomes are passed and every
cleanup reports zero remaining resources. A failed assertion, unsupported required facility, or
incomplete cleanup returns `NotReady` with stable scenario reasons. Ready evidence binds the
package manifest, scenario count, and digest of scenario and cleanup observations.

## Release integration boundary

Repository integration must add `peritus-platform-qualification` to the workspace and architecture
inventory, instantiate platform adapters in the H2 runner environment, stage binaries/helpers into
the target package paths, render the canonical manifest/checksums, authenticate the manifest
digest, and collect native runner evidence. Linux, macOS, and Windows results are independent: a
cross-compile or a platform-neutral test does not substitute for a fresh native packaged host.

The crate and assets define the acceptance mechanism. Their presence alone does not claim that a
package was built, installed, or qualified.
