# ARTEX managed application for Red Team

Status: implemented for review on `develop` (Linux native release path); real-interface acceptance
is recorded separately from this design.

## Objective and user outcome

An installed Red Team module offers **ARTEX integration** in the ordinary Peritus module menu. The
user can inspect the exact upstream license, explicitly accept it, install the pinned ARTEX release,
and inspect, start, stop, and read logs for the owned local instance without leaving Peritus.

ARTEX remains a separate application and license domain. Installing ARTEX does not authorize a
security target, configure an LLM, accept an ARTEX task, or extend Red Team's source authority.

## Source research and obligations

- Upstream: https://github.com/BaaRai/ARTEX
- Pinned release: `v0.5.1`, released from commit `ce279d7`.
- License: `AGPL-3.0-only`; exact v0.5.1 license SHA-256
  `0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0`.
- Linux release identities are copied from the upstream GitHub release metadata, not inferred from
  filenames. The host downloads over credential-free public HTTPS and verifies the exact archive
  SHA-256 before extraction.
- ARTEX requires PostgreSQL. The native adapter initializes a private user-owned cluster instead of
  invoking upstream's interactive installer or attempting privileged package installation.
- ARTEX's setup, authentication, LLM configuration, authorized target policy, and task semantics
  remain owned by ARTEX. This integration controls application lifecycle only.

## Ownership and trust boundaries

```text
user module action
  -> Red Team command adapter (no filesystem/process/network authority)
  -> module.external-application@1 request
  -> ProductRunner host adapter (live Read/Write/Process/Network checks)
  -> allowlisted ARTEX v0.5.1 descriptor
  -> private Peritus module-data installation and owned processes
```

The SDK adds only the capability name and the existing versioned request/result envelope. The host
accepts ARTEX requests only from module ID `peritus.red-team`, validates application identity and
operation, and supplies all version, URL, digest, license, path, port, and command values. Module
input cannot select executable bytes, a repository, a process, an endpoint, or an install path.

License acceptance is a separate Write-authorized operation bound to the exact version, SPDX
expression, upstream URL, and license digest. It performs no download or process effect. Install
requires a current matching receipt plus Read, Write, Process, and Network permission. Start and
stop recheck live authority. Stop signals only a retained PID whose `/proc` identity and working
directory match the owned ARTEX installation; a responder on port 8787 without that identity is
reported and never killed.

## Installation and lifecycle

The Linux adapter supports upstream's signed-release platforms `x86_64` and `aarch64`. It verifies
the corresponding release digest, bounds the download to 32 MiB, inventories every ZIP path before
extraction, rejects absolute/parent/backslash paths and oversized inventories, and smoke-tests the
release binary with `-h`.

The adapter resolves `unzip`, `initdb`, `pg_ctl`, `createdb`, `setsid`, and `kill` from the user's
current executable path and reports missing prerequisites before downloading. It reserves loopback
ports 8787/8788 and one private PostgreSQL port, initializes PostgreSQL with SCRAM authentication,
stores its random credential only in mode-0600 host state, writes ARTEX configuration with mode
0600, and atomically publishes the completed installation. Partial staging directories are not
recognized as installed instances.

Start launches the private database and ARTEX's official supervisor in a new process group with an
explicitly rebuilt environment. Provider keys and other ambient variables are not inherited. ARTEX
is bound to loopback; readiness is observed through `/api/health`. Stop sends SIGTERM to the owned
process group, waits for graceful exit without silently escalating to SIGKILL, then stops PostgreSQL.
Logs are returned as a bounded 64 KiB tail.

## Alternatives considered

1. **Run ARTEX's `install.sh` from the module.** Rejected: it is interactive, may install Docker
   through elevated effects, allows upstream behavior to change, and would give module code ambient
   process/network authority.
2. **Copy ARTEX's UI or engine into Peritus.** Rejected: it duplicates a separately licensed AGPL
   application, creates two competing execution authorities, and does not provide the requested
   actual ARTEX installation.
3. **Host-generic arbitrary installer descriptors.** Rejected for this version: an installed module
   could turn user selection into arbitrary downloaded code. The host-side allowlist is deliberately
   narrow until a separately reviewed signed integration-manifest contract exists.

## Compatibility and recovery

The capability is additive. Older Peritus builds do not advertise it; Red Team reports an update is
required and performs no fallback. Older Red Team builds ignore it. ARTEX state is versioned and
kept outside workspaces below Peritus's module data root. Restart recovery uses retained installation
and PID records plus live process/health observations. A stale license receipt, corrupt installation
record, PID mismatch, occupied port, checksum mismatch, unsafe archive, or failed prerequisite is a
visible rejection/failure, never an implicit reinstall or process kill.

## Acceptance criteria

- Before acceptance, `artex` shows the exact license identity and offers no install action.
- Accepting the exact displayed license records a receipt but downloads and starts nothing.
- A mismatched version/digest, wrong module ID, missing authority, unsafe archive, or occupied port
  is rejected without a recognized installation.
- Install verifies upstream bytes, creates private ARTEX/PostgreSQL state, and publishes the exact
  v0.5.1 installation identity.
- Start yields a healthy loopback ARTEX endpoint; stop removes health and stops the private database.
- Status and bounded logs are available through the same Red Team menu in TUI and WebUI.
- Existing Red Team investigation flows work unchanged when ARTEX is absent.

Verdict: ready for focused implementation review and real-interface qualification. This design does
not yet authorize ARTEX task creation or target control through its authenticated API; those need a
separate secure credential-input and target-authority design.
