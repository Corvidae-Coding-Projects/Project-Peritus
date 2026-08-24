# peritus-sandbox-macos

`peritus-sandbox-macos` is the C3 native enforcement adapter for macOS 15 and newer on Apple
Silicon and x86-64. It compiles a complete checked C2 sandbox plan into deterministic Seatbelt
profile text, a checksummed binary helper manifest, dimension-specific resource controls, and an
owned lifecycle/recovery session.

The crate is not a public process launcher. `peritus-process::ExecutionGateway` remains the only
authority and process-effect boundary. After validation and durable action consumption, C2 sends
the manifest over protected helper standard input, checks the fixed ready and digest-bound
activation records, owns the helper/target process group and PTY or pipes, and retains the session
through release. Commands are literal argv throughout; no shell string is constructed.

## Enforcement model

- The Seatbelt compiler emits `deny default`, explicit filesystem operations, process execution
  and descendant rules, explicit metadata denies, and proxy-only network output.
- `.git` and `.peritus` below the canonical workspace root are always protected. Additional
  protected roots may be supplied by installation policy. Denies dominate overlapping allows.
- Path text passes through one encoder. NUL/control-bearing or non-UTF-8 paths fail closed.
- The helper clears its inherited environment and restores only the exact non-secret assignments
  bound into the protected manifest, followed by checked protected secret/proxy deliveries.
- Whole-second CPU, address-space, and open-descriptor ceilings are installed as native rlimits.
  Fractional-second CPU, wall time, disk, output, process-tree count, and concurrency remain
  explicitly owned by the C2 supervisor; the backend does not overstate rounded `RLIMIT_CPU` or
  the per-user `RLIMIT_NPROC` facility as exact per-tree hard limits.
- Allowed UDP is unsupported. Allowed egress requires a checked loopback managed-proxy route.
- Recovery records contain only process/preparation/profile/helper identities, optional proxy
  route and secret-binding identity digests, lifecycle state, and cleanup progress. They never
  contain secret bytes, raw routing tokens, host-private paths, or operating-system handles.
- Managed proxy and secret configurations remain inert until C2 invokes the backend through its
  opaque authorized preparation callback. The resulting proxy, secret leases, and anonymous
  protected payload objects are owned through session release.
- The binary manifest names exact inherited descriptor numbers, labels, lengths, and checked
  destinations but contains no payload. C2 makes only those objects inheritable. The helper reads
  and zeroizes their contents before Seatbelt activation: proxy credentials become runtime-only
  `HTTP_PROXY`/`HTTPS_PROXY`, environment deliveries become exact target variables, file
  deliveries are privately created at exact destinations, and brokered deliveries remain on the
  manifest-whitelisted descriptor map.

## Probing and unsupported hosts

The runtime probe checks macOS product version, supported architecture, installed helper identity,
Seatbelt availability/profile compilation, process containment, PTY, resource-control mapping,
Keychain tooling, loopback managed-proxy transport, and any exact route supplied to the probe. The
descriptor advertises only the intersection actually enforceable and configured. Non-macOS hosts
return an empty unsupported probe instead of a reference or raw-effect fallback.

Platform-neutral compilation, manifest, lifecycle, and recovery tests run on every development
host. Native Seatbelt and rlimit behavior is compiled and executed only on macOS; Linux test runs
do not constitute macOS qualification.
