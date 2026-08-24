# C3 platform security backends

C3 turns the complete, checked C2 sandbox contract into native Linux, macOS, and Windows
enforcement. It also supplies the shared managed-network and secret-delivery owners used by those
backends. C3 grants no authority: the C2 execution gateway remains the only route from committed
B0/B1/B3/C0 authority to an operating-system process.

This is an implementation and operator guide. It does not claim that Peritus is released or that
the current host has passed packaged-host qualification.

## Authority and ownership sequence

Restricted execution follows one ordered path:

1. C2 checks the exact execution plan, checked sandbox plan, backend descriptor, support digest,
   preparation digest, committed authority receipts, and current platform.
2. C2 durably consumes the one-use action/process authority.
3. Only then does it construct an opaque `AuthorizedPreparationContext` and invoke the selected
   `NativeSandboxBackend`.
4. The backend prepares native resources, a managed proxy when required, exact secret deliveries,
   a bounded binary helper manifest, and a `NativeLaunchDescription`.
5. C2 starts the reviewed helper as its direct child, exchanges fixed ready and digest-bound
   activation records over ordinary pipes, and keeps PTY traffic on a separate C2-owned terminal.
6. The helper verifies the manifest and protected handles, installs every admitted control, and
   starts the literal executable and argv without a shell.
7. C2 supervises the complete tree and retains the native session until termination, proxy and
   secret release, platform cleanup, and terminal publication have all completed.

Descriptor, support, plan, preparation, helper, observation, or platform drift fails before target
activation. A failure after durable consumption becomes one durable non-success terminal result;
the consumed authority cannot be replayed.

The raw C2 launcher remains available only for plans explicitly authorized as raw effects. It is
not a fallback for a restricted plan whose native backend is unsupported.

## Crate responsibilities

| Crate | Responsibility |
|---|---|
| `peritus-process` | Sole launch gateway, helper framing, protected child-handle inheritance, PTY separation, tree ownership, cancellation, resource polling, and terminal publication |
| `peritus-network` | Checked network-plan narrowing, destination/DNS/redirect decisions, bounded HTTP/CONNECT proxy, scoped upstream credential injection, observations, and complete worker shutdown |
| `peritus-secrets` | Platform credential-store lookup, exact leases, zeroizing material, environment/file/brokered delivery ownership, redaction fingerprints, cleanup, and recovery records |
| `peritus-sandbox-linux` | Linux probe, namespaces/mounts, Landlock, seccomp, cgroup v2, rlimits, helper, recovery, and native observations |
| `peritus-sandbox-macos` | macOS probe, deterministic Seatbelt profile, process/PTY mapping, rlimits, helper, recovery, and native observations |
| `peritus-sandbox-windows` | Windows probe, path/ACL plan, restricted token/AppContainer, Job Object, inherited handles, ConPTY mapping, helper, recovery, and native observations |

All five C3 crates are runtime-layer, verification-class H roots. Their policy decisions and named
refinement relations are executable Verus Rust. Narrow operating-system and FFI adapters remain
ordinary Rust under the workspace unsafe inventory and documented safety contracts.

### Unsafe adapter inventory

The workspace denies unsafe code by default. C3 permits it only in these narrow modules, each with
a module-level safety contract and typed safe boundary:

| Module | Unsafe boundary |
|---|---|
| `peritus-process::platform::inheritance` | Unix `pre_exec` descriptor inheritance and Windows exact-handle inheritance flags |
| `peritus-network::proxy::inherited` | Unix borrowed/owned descriptor conversion around one checked `SCM_RIGHTS` listener transfer |
| `peritus-sandbox-macos::resource_monitor` | read-only libproc and rlimit resource queries |
| `peritus-sandbox-macos::runner` | Seatbelt, descriptor, rlimit, and process-launch system calls |
| `peritus-sandbox-windows::native` | restricted-token, AppContainer, Job Object, handle-list, ConPTY, ACL/path, process, and WFP calls |

No policy decision is delegated to these calls. Safe code constructs and validates the complete
plan first; adapters receive bounded owned inputs, convert platform failures to stable errors, and
return observations for C2 validation.

## Protected helper protocol

The helper command is a structured executable plus argv. The manifest is length-prefixed, bounded
to 4 MiB by C2, checksum-verified, and tied to the admitted preparation digest. It never contains
secret bytes, proxy routing tokens, host-private transient paths, or live OS handles as durable
authority.

`NativeProtectedHandle` owns either an anonymous staged payload or a pre-opened broker channel.
Its debug representation includes only a label and optional length. On Unix, C2 clears
close-on-exec only in the forked direct child. On Windows, C2 serializes the small inheritance
window, enables only the exact handle set, starts the child, and restores the parent flags. The
backend manifest binds the unchanged numeric handle to its exact nonsensitive role.

For PTY runs, stdin/stdout remain helper-protocol pipes. C2 supplies the already-open terminal
slave separately; the helper attaches it only to the target immediately before exec. Activation
records can therefore never appear in terminal output.

Target-start status is carried on a separate protected channel. Linux and macOS use a
manifest-bound close-on-exec descriptor: authenticated failure bytes identify helper exec failure,
while EOF proves that exec replaced the helper. Windows acknowledges only after suspended target
creation, Job assignment, relay setup, and resume. Target exit values therefore remain exact even
when they equal a helper's reserved pre-exec codes. A separate Windows terminal-control channel
carries bounded resize, interrupt, terminate, and input-close frames to the helper-owned ConPTY.

## Managed network

`NetworkPlan::from_checked` can only narrow an existing C2 contract. It binds owner and sandbox
digests, deny-dominant rules, DNS and redirect modes, HTTP/CONNECT mode, connection and aggregate
time/byte limits, worker and connection bounds, observation bounds, and scoped credential
references. UDP has no TCP fallback and is unsupported until a separate exact relay is admitted.

The proxy evaluates the requested DNS name or IP before resolution, validates every DNS answer,
and records the exact selected address. Explicit deny rules dominate. Loopback, unspecified,
multicast, link-local, private, and metadata-service destinations require explicit address-class
authority. Each new connection resolves again; an ambiguous or newly denied answer fails closed.

HTTP redirects are bounded and re-evaluated as successor destinations. CONNECT authorizes one
exact host and port. Proxy authorization is an opaque per-launch routing token, accepted either as
`Peritus <hex>` or as standard Basic proxy user-info `peritus:<hex>` for ordinary tools. The token
is installed by the helper at runtime, not stored in C2 environment planning or manifest bytes.
An upstream credential is acquired only after destination admission and an exact credential lease
use; it is stripped from proxy input and injected only into the matching outbound request.

Linux keeps a fresh network namespace without granting host networking. The helper binds a
loopback listener inside that namespace and passes its descriptor to the parent proxy owner over
one protected AF_UNIX channel. The parent accepts the namespace-bound listener descriptor, while
all upstream sockets are created by the parent proxy outside the target namespace. No veth,
privileged host network mutation, slirp fallback, or public listener is required.

The proxy owns its listener, cancellation token, bounded workers, relay threads, aggregate
accounting, observations, and credential revocation. Shutdown cancels active work and joins every
owned thread. An incomplete join is an incomplete teardown, never normal success.

## Secrets

`PlatformCredentialStore` uses the pinned keyring v1 adapter selected for Linux Secret Service,
macOS Keychain, or Windows Credential Manager. The deterministic memory store is compiled only for
tests or its explicit test feature. Store errors are reduced to stable missing, locked, denied,
stale, unavailable, corrupt, I/O, revoked, delivery, cleanup, or recovery classes without
retaining platform diagnostics that may contain sensitive data.

`SecretPreparation` is inert when configured. Inside the opaque authorized backend callback, it
matches the exact current owner, environment, sandbox digest, execution digest, reference,
destination, use count, and expiry; then it performs lookup and delivery. Missing, duplicate,
surplus, expired, or drifted leases fail closed and unwind partial work.

Secret material is non-Clone, redacted in `Debug`, and zeroized on drop. Delivery supports:

- environment values read by the helper from an anonymous handle and installed only immediately
  before target exec;
- private `0600` staging files mapped to the exact sandbox path and removed during release; and
- brokered handles inherited under one checked opaque label.

The delivery session owns all live material, leases, artifacts, and receipts. Release revokes
leases and drops successful artifacts. A file that could not be removed stays retained for a real
retry; the session cannot falsely report released while it remains present.

Routing tokens, upstream credentials, and secret values do not appear in canonical plans,
manifests, recovery records, observations, errors, terminal results, or ordinary debug output.

## Platform support and probes

### Linux

Production support requires x86-64 or AArch64 Linux 6.6 or newer, a reviewed installed bubblewrap
and Peritus helper identity, functional user/mount/PID/IPC/UTS/network namespaces, Landlock ABI 3
or newer, seccomp-BPF, delegated cgroup v2 controls required by the plan, PTY support when
requested, and the inherited-listener proxy bridge for egress. The helper self-attaches to the
prepared cgroup leaf and applies rlimits, Landlock, seccomp, privilege removal, and the exact
mount/network projection before target activation.

### macOS

Production support requires macOS 15 or newer on Apple Silicon or x86-64, the checked Peritus
helper, a functional system Seatbelt mechanism, process-group and requested PTY ownership,
required rlimits, credential-store access for secrets, and managed-proxy reachability for egress.
Profiles are deterministic implementation artifacts generated from canonical policy, with
deny-by-default rules, one path encoder, protected metadata precedence, and a preparation-bound
digest. Profile text grants no authority by itself.

### Windows

Production support requires Windows 11 24H2 or Windows Server 2025 on x86-64; the AArch64 build
path is retained where supported. The probe covers restricted primary tokens, low-integrity or
AppContainer support, kill-on-close Job Objects, exact handle-list behavior, ACL inspection and
reversal, ConPTY when requested, required resource controls, Credential Manager, and an admitted
managed network-control route. Device paths, alternate data streams, reserved names, reparse
escapes, and case-fold aliases are rejected before activation.

A backend advertises only the intersection proved on the current host. Missing facilities are
typed unsupported outcomes with corrective guidance; they do not silently select weaker controls.

## Cancellation, cleanup, and recovery

C2 owns the first cancellation trigger and the final process-tree reap. Native sessions receive
activation, cancellation, termination, resource-poll, and release callbacks in order. Hard native
limits and supervisor-enforced dimensions are reported separately; a supervisor crossing uses the
ordinary C2 resource-limit cancellation path.

Release covers the process tree, helper, proxy listener/connections/workers, routing token,
credential lease, secret destinations, cgroup/job/profile state, temporary ACLs, and support
threads. Holder quiescence remains blocked until all applicable owners report complete release.

Version-one recovery records are checksummed, nonsensitive evidence keyed to the C2 process and
native runtime identities. Recovery distinguishes exact live ownership, clean absence, mismatch,
and indeterminate state. It acts only on exact live resources; it never kills a reused PID/handle,
guesses success, or reconstructs secret bytes from a record.

## Validation

The repository CI matrix compiles, tests, lints, and documents the workspace on Ubuntu 24.04,
macOS 15, and Windows 2025. Platform-neutral descriptor, manifest, projection, recovery, and
conformance tests run everywhere; target-native cases are `cfg`-gated and never ignored. Linux
native enforcement tests run locally where the probe admits them. macOS and Windows native results
require their real runners; cross-compilation is build evidence, not packaged-host qualification.

Focused local commands are:

```text
CARGO_BUILD_JOBS=2 cargo test --package peritus-process --package peritus-network \
  --package peritus-secrets --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo test --package peritus-sandbox-linux \
  --package peritus-sandbox-macos --package peritus-sandbox-windows \
  --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy --package peritus-process --package peritus-network \
  --package peritus-secrets --package peritus-sandbox-linux \
  --package peritus-sandbox-macos --package peritus-sandbox-windows \
  --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" CARGO_BUILD_JOBS=2 cargo doc \
  --package peritus-process --package peritus-network --package peritus-secrets \
  --package peritus-sandbox-linux --package peritus-sandbox-macos \
  --package peritus-sandbox-windows --no-deps --all-features --locked
```

The complete merge authority remains `just gate-a`, including workspace policy, dependency policy,
ordinary-Rust API checks, no-cheating Verus verification/build, and the ordinary Rust gates. C3
completion makes C4 the next runtime boundary; it does not imply a release, H0 acceptance, or H2
packaged-host qualification.
