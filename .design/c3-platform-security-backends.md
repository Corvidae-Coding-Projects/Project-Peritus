# Feature: C3 platform security backends

- **Status:** frozen for implementation
- **Date:** 2026-08-24
- **Branch:** `feature/c3-platform-security-backends`
- **Crosslink issue:** `#12`

## Summary

C3 implements the native enforcement layer behind C2's frozen process and sandbox contracts. It
adds five runtime crates:

- `peritus-sandbox-linux`;
- `peritus-sandbox-macos`;
- `peritus-sandbox-windows`;
- `peritus-network`; and
- `peritus-secrets`.

The three platform crates compile one checked C2 sandbox plan into native preparation, activation,
observation, cancellation, and teardown behavior. `peritus-network` owns canonical connection
planning and a bounded managed proxy. `peritus-secrets` owns opaque secret lookup, zeroizing
material, delivery leases, redaction fingerprints, revocation, and cleanup. No C3 crate grants
authority or decides whether a run succeeded.

`peritus-process::ExecutionGateway` remains the sole process-effect and private-permit owner. C3
adds one narrow process-owned extension point: after the gateway validates the exact B0/B1/C0/B3
facts and durably consumes the action, it creates an opaque authorized preparation context and
invokes the selected native backend. The backend returns a supervised native session; it never
receives a public raw spawn API. C2 continues to own the child tree, PTY/pipes, cancellation,
resource observations, terminal result, recovery registry, output artifacts, and holder
quiescence.

Restricted launch is fail closed. A native backend advertises only controls proved present by its
runtime probe. Admission fails before durable consumption when required support is absent. A
failure after durable consumption produces a durable non-success terminal record and cannot make
the action replayable. Explicit raw effect remains a separate B1 operation and is never selected as
a fallback.

Pure matching, plan projection, preparation validation, lifecycle reduction, resource accounting,
secret-delivery matching, observation validation, and fail-closed decisions are executable Verus
Rust. Operating-system calls, credential stores, proxy sockets, and helper processes remain narrow
H/T effect modules with bounded typed observations. Platform FFI is permitted only in named,
inventoried modules with explicit safety contracts; the rest of the workspace remains unsafe-free.

## User-visible behavior

1. A restricted execution selects one native backend whose exact descriptor, support set, probe,
   checked sandbox plan, and preparation digest are bound into the existing C2 execution plan.
2. On Linux, the child runs inside a new namespace/mount boundary with explicit filesystem views,
   metadata roots hidden, Landlock and seccomp applied when required, cgroup-v2 resource controls,
   and a process tree still owned by C2.
3. On macOS, the child runs through a generated Seatbelt profile with explicit filesystem and
   network rules, protected metadata, process-group containment, bounded resources, and C2-owned
   PTY/pipe lifecycle.
4. On Windows, the child runs with a restricted token or AppContainer profile as selected by the
   checked requirements, an owned job object, exact ACL/path handling, resource controls, and
   fail-closed network enforcement.
5. Network access is denied by default. An allowed connection is made through a bounded managed
   proxy that rechecks the requested host, resolved addresses, transport, port, redirects, and
   duration and emits normalized plan-bound observations.
6. A narrowly scoped proxy credential can be injected into the outbound request without appearing
   in the child environment, command, plan, log, error, observation, or artifact.
7. Secret values are obtained only for exact checked `SecretReference` values and delivered only
   to their declared environment, file, or brokered-handle destinations. Values are zeroized and
   revoked or removed during teardown.
8. Missing kernel services, disabled user namespaces, unavailable Seatbelt/AppContainer controls,
   absent credential stores, undelegated cgroups, and unavailable network isolation produce stable
   actionable unsupported results rather than weaker execution.
9. Native lifecycle and enforcement observations are ordered, bounded, digest-bound, and included
   in the existing C2 terminal and recovery accounting. Cleanup failure is visible and prevents a
   false complete-teardown or holder-quiescence claim.
10. All native backends implement the same A2 C2/C3 sandbox conformance contract. Platform-specific
    tests additionally prove real host behavior where that platform is executing.

## Requirements

### R-C3-001 — dependency direction and sole effect authority

The dependency direction is:

```text
peritus-network         -> peritus-sandbox, peritus-types, peritus-codec, vstd
peritus-secrets         -> peritus-sandbox, peritus-types, peritus-codec, vstd

peritus-sandbox-linux   -> peritus-process, peritus-sandbox, peritus-network,
                           peritus-secrets, peritus-types
peritus-sandbox-macos   -> peritus-process, peritus-sandbox, peritus-network,
                           peritus-secrets, peritus-types
peritus-sandbox-windows -> peritus-process, peritus-sandbox, peritus-network,
                           peritus-secrets, peritus-types
```

`peritus-process` and `peritus-sandbox` never depend on a C3 implementation crate. C3 cannot
construct `ExecutionPermit`, `AuthorizedLaunch`, B0/B1/C0 receipts, terminal success, acceptance,
or holder-quiescence evidence. A native backend is called only from the exact C2 gateway after
authorization and durable consumption.

### R-C3-002 — process-owned native backend seam

C2 adds a public, implementation-neutral `NativeSandboxBackend` trait and opaque
`AuthorizedPreparationContext`. `ExecutionGateway::launch_with_backend` accepts the ordinary
unprivileged authorization request, `ExecutionPlan`, exact `CheckedSandboxPlan`, exact
`BackendAdmission`, and one backend implementation.

The gateway validates all existing C2 facts plus:

- checked-plan digest equals the execution plan's sandbox digest;
- admission plan/descriptor/support/preparation digests equal the execution plan selection;
- the backend's probed descriptor exactly equals the admitted descriptor;
- the backend implementation identity and platform equal the selected backend;
- every native launch input names only requirements already present in the checked plan; and
- no preparation or activation effect occurs before complete validation and durable consumption.

Only then does the gateway create its private permit and opaque preparation context. The context
has safe read-only accessors for the exact execution and sandbox plans and one consuming `finish`
operation that binds the returned native session to the permit, plan, descriptor, and preparation
digest. Callers cannot construct the context or a supervised native session directly.

### R-C3-003 — supervised native lifecycle

A prepared native session supplies a structured wrapper/helper launch description and implements
bounded hooks for `prepared`, `activated`, `cancel_requested`, `terminated`, and `released`.
`peritus-process` invokes these hooks in its existing owner thread and retains the session until
release. The backend does not detach work or outlive `OwnedProcess`.

The session returns typed `EnforcementObservation` values for every required domain. C2 validates
sequence, plan/preparation/descriptor digests, capability domain, lifecycle phase, resource
dimension, and teardown completeness before terminal publication. Backend errors map to sandbox
denial, unsupported, resource limit, supervisor failure, or recovery-indeterminate; none can become
normal exit merely because the wrapped command exits zero.

### R-C3-004 — native helper protocol

Each platform crate owns a small helper binary used as the direct C2 child. The helper receives a
versioned bounded binary preparation manifest through a protected inherited handle, never through
a shell or model-visible command string. It verifies the manifest checksum and exact preparation
digest, installs native controls, emits a fixed-size activation record, and `exec`s or spawns the
literal target argv.

Secret bytes are carried only by protected anonymous/shared-memory handles owned by the prepared
session. They never enter argv, the manifest, environment planning bytes, or a filesystem path
name. The helper closes every unrelated descriptor/handle before target execution. Failure before
target activation exits with a reserved helper category that C2 maps to `SandboxDenied` or
`SupervisorFailed`, not target exit.

### R-C3-005 — Linux backend

`peritus-sandbox-linux` supports x86-64 and AArch64 Linux with kernel 6.6 or newer, cgroup v2, and
the platform facilities required by the exact plan. Its production restricted profile uses:

- user, mount, PID, IPC, UTS, and network namespaces through an installed reviewed `bubblewrap`
  runner or equivalent direct safe adapter;
- an explicit read-only host view plus exact writable mounts, with `/proc` and temporary roots
  created inside the namespace;
- `.git`, `.peritus`, harness policy, evaluator, approval, and secret roots absent or read-only;
- Landlock ABI 3 or newer for a second filesystem layer when the plan requires mutation controls;
- a seccomp-BPF allow policy generated by a reviewed fixed syscall-class compiler;
- a delegated cgroup-v2 leaf for memory, process count, and CPU controls, plus C2 supervision for
  dimensions without truthful hard host enforcement;
- no ambient capabilities, `no_new_privs`, bounded rlimits, and exact environment clearing; and
- a network namespace with only the managed proxy route when egress is allowed.

The runtime probe reports kernel version, Landlock ABI, namespace availability, bubblewrap/helper
identity, seccomp availability, cgroup-v2 delegation, PTY support, and managed-network reachability.
It advertises only the intersection actually enforceable on the current host.

### R-C3-006 — macOS backend

`peritus-sandbox-macos` supports macOS 15 and newer on Apple Silicon and x86-64. It compiles a
deterministic Seatbelt profile from the complete filesystem, process, network, and metadata policy;
launches through the checked system Seatbelt mechanism without a shell; uses C2 process-group/PTY
ownership; applies safe resource limits available to the helper; and routes allowed egress only
through the managed proxy.

Profile text is an implementation artifact, never authority. It is generated from canonical plan
data, escapes paths through one tested encoder, contains deny-by-default rules, and is digest-bound
to preparation. The probe checks OS version, Seatbelt executable/API availability, profile
compilation, process containment, PTY, resource controls, credential-store access, and proxy
reachability. Missing controls are unsupported.

### R-C3-007 — Windows backend

`peritus-sandbox-windows` supports Windows 11 24H2 and Windows Server 2025 on x86-64, with an
AArch64 build path retained where the toolchain supports it. It uses a restricted primary token and
low-integrity/AppContainer isolation where required, a kill-on-close job object, explicit desktop
and inherited-handle policy, exact workspace ACL grants, Windows path normalization, reparse-point
checks, and managed network enforcement.

The helper never mutates broad user or repository ACLs. Temporary grants are exact, recorded, and
reverted during release. Device paths, alternate data streams, reserved names, junction escapes,
and case-fold aliases are rejected or resolved before activation. The probe reports token,
AppContainer, job, ACL, ConPTY, resource, credential-manager, and network-control availability.

### R-C3-008 — filesystem and protected metadata projection

Every backend maps each C2 filesystem operation separately. Discovery/metadata/read/execute and
create/write/remove are not collapsed into one broad read/write bit. Deny rules dominate. The
workspace root is resolved before authorization and revalidated during preparation. Protected
metadata roots remain absent or read-only even when an overlapping workspace grant is writable.

Preparation rejects an unrepresentable path, alias collision, mount/volume mismatch, or policy that
the platform cannot encode exactly. Native enforcement observations name only canonical path IDs or
digests; they do not leak host-private paths unnecessarily.

### R-C3-009 — process, terminal, and resource projection

Native helpers may start only the exact root executable and descendants permitted by the C2
process contract. C2 remains the tree owner. Platform containment must cover the helper, target,
and descendants for both pipes and PTY. Signal and resize operations remain mediated by
`ProcessControl`.

Each resource dimension reports one of hard enforcement, supervisor enforcement, unsupported, or
incomplete. Claims are dimension-specific even where C2 retains an aggregate backend fidelity for
compatibility. Required hard enforcement missing from a host fails before activation. Cleanup waits
for tree quiescence, helper exit, proxy/secret lease release, cgroup/job/profile teardown, and all
support threads.

### R-C3-010 — verified network request plan

`peritus-network` converts the checked sandbox network requirements into one canonical
`NetworkPlan`. It binds plan/owner IDs, rules, transport, DNS mode, redirect mode/count, connection
and total duration, byte ceilings, observation ceiling, credential references, and proxy mode.

Host matching is case-normalized and deny-dominant. DNS names, exact/suffix selectors, IP prefixes,
IPv4/IPv6 representations, ports, and transports use typed values. A lower-authority request cannot
broaden a C2 rule. Canonical bytes are order-independent for policy sets and field-sensitive for
every authority-relevant value.

### R-C3-011 — DNS, redirect, and destination enforcement

The proxy resolves DNS itself under the plan's resolution mode. It checks the requested DNS name,
every resolved address, and the selected connect address. Loopback, unspecified, multicast,
link-local, private, and metadata-service destinations are denied unless the checked plan contains
an exact explicit grant for that address class.

HTTP redirects are re-evaluated as new destinations and bounded by count and total duration.
`CONNECT` tunnels are limited to the exact admitted host/port and never interpreted as a wildcard.
DNS rebinding is handled by retaining and observing the exact selected address for the connection;
later connections resolve and check again.

### R-C3-012 — bounded managed proxy

`ManagedProxy` owns one loopback listener, a bounded connection queue, bounded worker count,
cancellation, per-connection and total byte/time accounting, and every join handle. It supports
HTTP forwarding and HTTPS `CONNECT` tunnelling without TLS interception. UDP is unsupported unless
a separately implemented exact datagram relay is admitted; it never rides a TCP success path.

The proxy emits monotonically sequenced normalized observations containing plan digest, requested
host, selected address, port, transport, decision, redirect depth, byte counts, timing, credential
injection marker, and terminal class. Payload bodies and credentials are not observation data.
Dropping the owner cancels, closes the listener and connections, joins workers, and reports
incomplete teardown when a bounded join fails.

### R-C3-013 — scoped proxy credential injection

A proxy credential lease binds one secret reference to an exact host matcher, transport, port,
header name or connection mode, use count, expiry, network-plan digest, and owner lineage. The proxy
requests material only after destination admission and injects it only into the matching outbound
request. Redirects do not carry credentials unless the successor independently matches the lease.

Child processes receive only the proxy endpoint and an opaque per-launch routing token. They never
receive the upstream credential. Routing tokens are random, bounded, revocable, and compared in
constant time where secret-dependent comparison is required.

### R-C3-014 — secret references and stores

`peritus-secrets` maps each C2 `SecretReference` to a validated provider and versioned entry. The
production adapters use Linux Secret Service, macOS Keychain, and Windows Credential Manager
through the pinned `keyring` implementation. A deterministic memory store exists only for tests and
is marked non-production.

Lookup requires an exact `SecretLease` containing owner lineage, environment/resource, checked
sandbox and execution-plan digests, reference/version, delivery destination, use count, and expiry.
Store errors distinguish missing, locked, denied, stale version, unavailable, corrupt, revoked, and
I/O failure with stable recovery guidance.

### R-C3-015 — zeroizing material and delivery

Secret values live in a non-`Clone`, redacted-`Debug`, zeroizing `SecretMaterial`. Empty and
oversized values are rejected. The value is never serializable, hashable, comparable by ordinary
equality, or exposed as a string. A closure-based `expose` method limits borrowing to the delivery
operation.

`SecretDeliverySession` consumes exact leases and materializes only the C2-declared destination:

- environment delivery is installed by the native helper immediately before target execution;
- file delivery uses a platform-protected anonymous or private file mounted/materialized at the
  exact sandbox path and removed during release; and
- brokered-handle delivery passes one non-inheritable-by-default descriptor/handle made inheritable
  only for the exact child and closed by both sides after use.

Partial delivery triggers revocation and cleanup of every already-created destination. Repeated
release is idempotent. Cleanup failure is observable and prevents complete teardown.

### R-C3-016 — redaction fingerprints and leak detection

Each active material produces keyed, domain-separated redaction fingerprints for exact value and
bounded fragments without storing reversible secret data. Fingerprints can test candidate bytes
through a constant-time digest comparison and expire with the lease. Ordinary debug, error, plan,
observation, terminal, artifact, and recovery representations contain only references, versions,
destinations, lengths where policy permits, and redaction markers.

Seeded canary tests inspect canonical bytes, `Debug`, `Display`, errors, proxy observations, sandbox
observations, terminal results, process manifests, artifacts, helper manifests, and cleanup reports.

### R-C3-017 — recovery and cleanup

Per-backend protected runtime records are versioned, checksummed, and keyed by C2 `ProcessId` and
preparation digest. They record only nonsensitive native resource identities: cgroup path ID,
namespace/helper identity, profile digest, job/AppContainer identity, proxy routing ID digest,
secret lease references, activation state, and cleanup progress.

On reopen, exact native resources are classified as live-owned, absent-clean, mismatched, or
indeterminate. Only exact live-owned resources may be terminated or cleaned. Missing records,
identity reuse, inaccessible OS state, or cleanup ambiguity remain indeterminate and block C2
quiescence. Secret values are never recovered from C3 records; the credential store is re-queried
only through a still-valid exact lease.

### R-C3-018 — stable errors and observations

Every crate exposes typed errors with stable subsystem code, operation, bounded detail, and recovery
class. Normal environment failure does not panic. Categories cover invalid plan, unsupported host,
probe failure, descriptor mismatch, preparation mismatch, native helper failure, sandbox denial,
path/ACL/mount/profile/token/cgroup/job failure, DNS/redirect/proxy failure, credential failure,
secret delivery/revocation/cleanup failure, observation mismatch, and recovery indeterminacy.

Native observations are bounded and canonical. They report facts, never acceptance. A backend
cannot claim a feature it did not probe and activate; C2 rejects missing, duplicate, out-of-order,
cross-plan, cross-process, and post-release observations.

### R-C3-019 — formal obligations

C3 registers the following `in-progress` obligations with exact proof symbols and executable
refinement tests:

- `OBL-0130`: an admitted native backend covers every required C2 feature and every emitted native
  session remains bound to the exact plan, descriptor, probe, and preparation;
- `OBL-0131`: a network decision or connection is no broader than the checked C2 network contract,
  including resolution and redirects;
- `OBL-0132`: a secret delivery implies one exact live lease/reference/destination and ordinary
  representations exclude the secret material;
- `OBL-0133`: complete native teardown implies zero owned backend/proxy/secret resources remain;
  and
- `OBL-0134`: unsupported or mismatched native preparation has no process, network, or secret
  activation effect.

No existing reservation or obligation is removed. Deterministic predicates live in verified
modules; OS observations are tested refinement inputs, not trusted desired outcomes.

### R-C3-020 — maintainability and repository integration

All five crates use composition-only roots, private fields, responsibility-based modules, typed
public APIs, stable docs, and no reachable placeholders or ignored tests. Platform-specific code is
compiled only behind target cfgs while platform-neutral descriptor, manifest, probe, and adapter
logic compiles on every tier-one target.

Cargo, lockfile, architecture, dependency execution review, duplicate-version policy, unsafe/TCB
inventory, strict Verus lists, CI platform checks, A2 catalogs, `justfile`, formal documentation,
C3 operator documentation, and the root README register the complete slice. The README reports C3
implemented and C4 as the next runtime boundary without claiming product release or H0/H2
qualification.

## Acceptance criteria

1. All five C3 crates exist, are owner C3/layer runtime, declare truthful V/H/T classification,
   build in ordinary Rust, and include crate docs, README, typed errors, and responsibility modules.
2. C2 exposes exactly one native extension route. API tests prove restricted execution cannot use
   the raw launcher, callers cannot construct authorized contexts/sessions, and every pre-authority
   native callback count remains zero.
3. Backend drift tests independently change plan, descriptor, support, probe, preparation,
   platform, secret, and network bindings; every mismatch produces no activation.
4. Real Linux tests prove literal argv, read-only/read-write mount behavior, protected metadata,
   descendant containment, network namespace/proxy-only egress, Landlock/seccomp activation,
   cgroup resource overrun, cancellation, PTY, teardown, and unsupported fail-closed behavior.
5. Linux probe tests cover missing bubblewrap/helper, disabled namespaces, insufficient Landlock,
   seccomp absence, undelegated cgroups, and missing proxy route without false support claims.
6. macOS target tests prove canonical Seatbelt profile generation, path escaping, deny dominance,
   metadata protection, network/proxy rules, process/PTY/resource mapping, probes, helper protocol,
   and teardown. Native tests execute when the target runner is available.
7. Windows target tests prove path/reparse/ADS/device rejection, exact ACL planning/reversal,
   restricted-token/AppContainer/job mapping, inherited-handle policy, ConPTY/resource/network
   probes, helper protocol, and teardown. Native tests execute when the target runner is available.
8. Every native adapter runs the complete fresh-subject A2 sandbox suite. Platform-independent
   preparation/conformance tests run on the local host; target-native enforcement cases are cfg
   gated, never ignored.
9. Network unit/property tests cover exact/suffix/IP-prefix matching, deny precedence, canonical
   order, IPv4/IPv6, private/special address policy, DNS multi-answer selection, rebinding, redirects,
   CONNECT, duration, bytes, connection/worker bounds, cancellation, and observation ordering.
10. Real loopback proxy tests cover allowed HTTP, allowed CONNECT tunnel, denied destination,
    redirect revalidation, scoped credential injection/non-forwarding, backpressure, shutdown, and
    complete worker joins without public Internet access.
11. Secret tests cover every store outcome, exact lease matching, expiry/use/revocation, all three
    delivery modes, partial-failure cleanup, idempotent release, zeroization instrumentation, and
    platform adapter capability probes.
12. Seeded canaries are absent from every ordinary representation and persisted artifact named in
    R-C3-016, including failure and cleanup paths.
13. Lifecycle/recovery tests reopen every native phase, classify exact live/absent/mismatch/
    indeterminate resources, signal or clean only exact ownership, and block quiescence until
    complete backend/proxy/secret release.
14. Named Verus proofs and real negative refinement tests register `OBL-0130` through `OBL-0134`
    with strict no-cheating verification.
15. No unrelated refinement reservation or existing obligation is removed or weakened.
16. New unsafe code, if unavoidable, exists only in checked platform/FFI paths with module safety
    contracts, architecture registration, and focused tests. The repository unsafe inventory is
    otherwise unchanged.
17. Dependency versions are exact, default features are disabled unless reviewed, build scripts
    and procedural macros are registered, licenses/sources pass, and no external implementation
    type leaks through a public API.
18. Formatting, focused tests, strict Clippy/rustdoc, cross-target Linux/macOS/Windows checks,
    architecture/source/trust/API/reproducibility checks, full Verus verification/build,
    `just check`, and local `just gate-a` pass.
19. One bounded independent cross-review finds no unresolved concrete authority, enforcement,
    lifecycle, cleanup, recovery, confidentiality, or maintainability defect.
20. Root README and C3 operator documentation accurately describe implemented behavior, platform
    requirements, capability probes, unsupported routes, recovery, and the remaining release work.
21. The final selected diff contains only project-owned C3 changes and no provider/Crosslink/user
    configuration.
22. A signed conventional source commit is pushed and merged through a protected-main PR; its hash
    is an ancestor of exact `origin/main`; temporary bypass is removed; all repository protections
    are restored; issue `#12` and its session/lock are closed with result evidence.

## Current architecture

C2 is merged at `origin/main` `7003a43664164e55a29f9b14f0239409594e4793`. It supplies a
complete seven-domain `SandboxContract`, `SandboxRequirements`, deterministic
`CheckedSandboxPlan`, exact `BackendDescriptor` and `BackendAdmission`, executable reference
backend, structured `ExecutionPlan`, sole `ExecutionGateway`, private permit and authorized launch,
owned pipe/PTY supervisor, resource/output accounting, durable process recovery, and A2 process and
sandbox conformance.

The sandbox values already expose canonical rules and requirements needed by C3. Secret plans hold
only `SecretReference` and `SecretDelivery`, never values. Network plans hold DNS/IP targets,
transport, and port but do not yet express proxy/DNS/redirect/duration details; C3 adds a narrower
runtime plan bound to the checked C2 policy without changing or broadening C2 canonical bytes.

The actual C2 gateway currently permits only `ExplicitRawEffect` and calls a crate-private local
platform launcher. Its native authorized-launch type and supervisor entry are crate-private. This
is the intended pre-C3 gate, but the external native seam promised by the C2 design is not yet
implemented. C3 owns that narrow additive public interface and its no-bypass tests.

A2 already labels its sandbox subject as C2/C3 and supplies ten fresh-subject cases for default
denial, filesystem, environment/secrets, network, process/terminal, resources, unsupported
admission, cancellation/teardown, observation binding, and canonical preparation. C3 extends the
observation contract where necessary and implements adapters; A2 never depends on a runtime crate.

The workspace pins Rust `1.97.1`, Verus `0.2026.08.09.92f466f`, and forbids unsafe globally.
Current C3 dependency candidates are `landlock = 0.4.7`, `seccompiler = 0.5.0`,
`zeroize = 1.9.0`, `keyring = 4.1.6`, and `windows-sys = 0.61.2`, all exact and with reviewed
target-specific features only. The local Linux host has kernel 7.1, cgroup v2, bubblewrap, and a
Windows GNU Rust target. CI already names Ubuntu 24.04, macOS 15, and Windows 2025.

No C3 stored schema or public release exists, so the new native/proxy/secret runtime records begin
at version one. Existing C2 canonical plan and terminal schemas remain readable and are not
reinterpreted.

## Proposed design

### Preferred architecture

The preferred design is a process-owned callback seam. The gateway authorizes and consumes first;
only its opaque context can ask an admitted backend to prepare a session. The supervisor launches a
structured helper description and owns the returned native session through release. This preserves
C2's authority and lifecycle code, keeps OS crates outward dependencies, and makes lying or partial
backend observations testable at one reducer boundary.

Platform helpers are used because namespace/profile/token setup must occur immediately around
target creation and differs sharply by OS. Helpers receive a binary manifest and protected handles,
not shell text. The helper remains the direct C2 child, so existing process-group/job and PTY/pipe
ownership cover the target and descendants.

### Alternative: platform-owned public spawn

Each platform crate could expose `spawn(command, policy)` and return a process handle. That is easy
to implement independently, but it duplicates authorization, consumption, C2 output/resource
supervision, recovery, and terminal classification and makes raw bypass public. It is rejected.

### Alternative: container-only backend

Requiring Docker/Podman would offer useful isolation on some Linux hosts but would not implement
native macOS/Windows policy, local credential stores, or stable host path semantics. Containers may
be added later as another descriptor; they do not replace the three tier-one native backends.

### Modules

`peritus-network`:

```text
src/lib.rs
src/error.rs
src/policy.rs
src/plan.rs
src/canonical.rs
src/matcher.rs
src/resolution.rs
src/redirect.rs
src/accounting.rs
src/cancellation.rs
src/credential.rs
src/observation.rs
src/proxy.rs
src/proxy/accept.rs
src/proxy/http.rs
src/proxy/connect.rs
src/proxy/worker.rs
src/proxy/owner.rs
src/recovery.rs
src/verified.rs
src/refinement.rs
```

`peritus-secrets`:

```text
src/lib.rs
src/error.rs
src/reference.rs
src/material.rs
src/lease.rs
src/store.rs
src/store/memory.rs
src/store/platform.rs
src/delivery.rs
src/delivery/environment.rs
src/delivery/file.rs
src/delivery/handle.rs
src/redaction.rs
src/revocation.rs
src/cleanup.rs
src/recovery.rs
src/verified.rs
src/refinement.rs
```

Each platform crate uses the same responsibility names, with platform-specific implementation
inside them:

```text
src/lib.rs
src/error.rs
src/descriptor.rs
src/probe.rs
src/manifest.rs
src/canonical.rs
src/preparation.rs
src/session.rs
src/runner.rs
src/filesystem.rs
src/process.rs
src/resource.rs
src/network.rs
src/secret.rs
src/observation.rs
src/recovery.rs
src/conformance.rs
src/verified.rs
src/refinement.rs
src/bin/peritus-<platform>-sandbox-helper.rs
tests/contracts.rs
tests/conformance.rs
tests/native_enforcement.rs
```

Modules split before the repository 700-line hard limit. Target-only details stay below one named
module rather than spreading `cfg` branches throughout public types.

### Parallel ownership

After this design freezes:

| Lane | Exclusive paths | Shared files prohibited |
|---|---|---|
| Linux | `crates/runtime/peritus-sandbox-linux/**` | C2, A2, root manifests, other C3 crates |
| macOS | `crates/runtime/peritus-sandbox-macos/**` | C2, A2, root manifests, other C3 crates |
| Windows | `crates/runtime/peritus-sandbox-windows/**` | C2, A2, root manifests, other C3 crates |
| Integration | network, secrets, C2/A2 seams, design, Cargo/lock, architecture, verification, CI, xtask, docs | platform internals except coordinated review fixes |

Platform lanes implement against the signatures frozen here and may use crate-local target adapters
until the integration seam lands. Only the integration owner resolves shared API or manifest
changes.

## Data and compatibility

Native preparation manifests, proxy recovery records, secret lease records, and platform cleanup
records begin at schema version one. Canonical encodings use fixed domains, discriminants,
big-endian lengths/integers, bounded collections, and complete authority-field coverage. Raw secret
bytes, credential values, routing tokens, host-private temporary paths, and OS handles are excluded
from canonical/durable records.

Existing C2 `ExecutionPlan` and `TerminalResult` version-one bytes remain unchanged. Native
observations occupy existing sandbox/resource observation slots where possible. Any additional C2
terminal field is additive only through a new versioned encoding with the old decoder retained.

The C3 runtime registry is supporting recovery evidence, not a second authority store. C0 remains
authoritative. A native record cannot grant permission, mark acceptance, or override a C2 terminal.

## Failure handling

- Invalid or broadened network/secret/native inputs fail before backend preparation.
- Missing native support fails before durable consumption.
- Gateway mismatch invokes no backend, proxy, store, or helper effect.
- Preparation failure after durable consumption records one non-success terminal result and cleans
  partial native resources.
- Helper activation failure terminates the helper tree, revokes secret/proxy leases, and reports
  the exact reserved helper failure category.
- Network denial closes the attempted connection and records a bounded decision; it never retries
  outside policy.
- DNS or redirect ambiguity is denial/unsupported, not fallback.
- Credential-store or secret-delivery failure exposes no material and unwinds already-created
  destinations.
- Cancellation reaches child tree, helper, proxy connections, secret delivery, and platform
  resources under one owned session.
- Cleanup uncertainty yields incomplete/indeterminate terminal and blocks holder quiescence.
- Recovery acts only on exact matching native identities and never guesses success or kills a
  reused resource.

All errors include a stable code and one actionable route: correct request, select supported
backend, reauthorize, unlock/configure credential store, enable/delegate platform service, cancel
and reap, reopen and reconcile, retry cleanup, or quarantine.

## Security considerations

C3 enforces ordinary production isolation; it does not rely on prompts. The model cannot choose a
backend descriptor, implementation, secret value, proxy credential, native helper, or raw fallback.
The same canonical C2 plan drives authorization, native preparation, display, and observations.

Backend/helper manifests are data, never shell input. Helper paths and versions are installation
configuration checked against the selected descriptor. Sensitive material moves only through
protected handles and zeroizing buffers. Secret values are absent from hashes because keyed
redaction fingerprints, not raw digests, are the matching surface.

Native implementations are TCB effect boundaries, but their deterministic policy decisions remain
verified. Unsafe FFI is not spread across the workspace: the root lint becomes `deny`, platform FFI
modules locally allow it, and `xtask` rejects unsafe anywhere except the exact registered paths.
Each allowed module documents pointer, handle, buffer, thread, inheritance, and teardown invariants.

This slice covers realistic normal production and failure paths. The later H0 independent
escape-focused review and H2 full packaged platform qualification remain distinct evidence gates;
C3 does not use that boundary to omit native behavior or common conformance.

## Verification

Focused checks include:

```text
cargo test --package peritus-network --package peritus-secrets --all-targets --all-features --locked
cargo test --package peritus-sandbox-linux --all-targets --all-features --locked
cargo test --package peritus-conformance --package peritus-process --package peritus-sandbox --all-targets --all-features --locked
cargo clippy --package peritus-network --package peritus-secrets --package peritus-sandbox-linux --package peritus-process --package peritus-sandbox --package peritus-conformance --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-network --package peritus-secrets --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-process --package peritus-sandbox --no-deps --all-features --locked
cargo check --package peritus-sandbox-macos --target x86_64-apple-darwin --all-targets --all-features --locked
cargo check --package peritus-sandbox-macos --target aarch64-apple-darwin --all-targets --all-features --locked
cargo check --package peritus-sandbox-windows --target x86_64-pc-windows-gnu --all-targets --all-features --locked
cargo verus verify --package peritus-network --package peritus-secrets --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-process --package peritus-sandbox --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
cargo run --locked --package xtask -- all
just check
just gate-a
```

macOS/Windows native tests are ordinary cfg-target tests, not ignored tests. They execute in the
configured CI matrix when hosted runners become available. Local completion requires their
platform-neutral tests and feasible cross-target check/Clippy plus real Linux enforcement on the
current host. This is reported truthfully and is not presented as H2 qualification.

## Rollout and rollback

C3 lands as all five crates, the one C2 native seam, A2 extensions/adapters, formal obligations,
policy/toolchain registration, tests, and documentation in one protected-main PR. No partial crate
is advertised or released.

Before C4 consumes C3, rollback can remove the C3 crates and additive native seam while retaining
C2 raw/reference operation. After a downstream consumer persists C3 record version one, rollback
retains readers and produces explicit unsupported results instead of reinterpreting records.

Delivery completes only after the full completion audit, bounded cross-review, focused/platform
checks, local Gate A, signed commit, push, merged PR, signed-source ancestry verification,
protection restoration, and Crosslink closure.

## Open questions

None block implementation. The process-owned native callback seam, helper protocol, platform
minimums, managed-proxy boundary, secret-store choice, canonical record ownership, verification
obligations, and parallel paths are frozen by this design.

## Out of scope

- Model-facing filesystem, shell, Git, and quality tools are C4.
- Complete daemon startup/shutdown composition is G0; C3 supplies owned sessions and recovery
  primitives consumed there.
- Independent sandbox escape review, full packaged platform parity, soak/load evidence, and release
  qualification are H0/H2/H3/H4. They are qualification gates, not missing C3 implementation.
- Public Internet tests are unnecessary. Network integration uses controlled loopback endpoints.
- A general container runtime is not implemented. A container may be another backend later.

## Architecture verdict

`ready`

The design preserves C2 as the only authority and process-effect owner, gives each native platform
an independent implementation boundary, provides real managed network and secret services, maps
every explicit C3 requirement to observable evidence, and leaves no product decision unresolved.
