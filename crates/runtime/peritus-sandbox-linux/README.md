# peritus-sandbox-linux

`peritus-sandbox-linux` is the Linux C3 native enforcement adapter. It converts an already checked
C2 sandbox plan into a deterministic bubblewrap mount/namespace plan, a bounded helper manifest,
dimension-specific resource controls, ordered observations, and a recoverable cgroup lifecycle.
It grants no authority and has no raw process-spawn fallback.

Production admission requires Linux 6.6 or newer on x86-64 or AArch64 and advertises only controls
proved by the runtime probe. Required facilities include functional user/mount/PID/IPC/UTS/network
namespaces through the configured bubblewrap executable, Landlock ABI 3 or newer, seccomp-BPF,
PTY support, and delegated cgroup v2 controllers. Egress uses an inert managed-proxy preparation:
the helper binds loopback inside its fresh network namespace and transfers that listener over an
exact inherited Unix channel to the parent proxy owner, whose upstream connects remain outside the
namespace. Missing support is a typed, fail-closed result.

The direct child is bubblewrap, which starts `peritus-linux-sandbox-helper`. The literal target
program and arguments occur only in a version-one checksummed manifest supplied through C2's
protected anonymous protocol pipe. The helper first proves channel readiness, then verifies the
framed manifest and its launch-bound manifest and preparation digests. It self-attaches to the
exact prepared cgroup leaf, applies rlimits, Landlock and seccomp, emits the digest-bound activation
record, and executes the literal target without a shell. For PTY execution C2 keeps the protocol
pipes separate and supplies the process-owned PTY slave through its reserved one-shot attachment.
Exact secret leases are resolved only inside authorized preparation; their environment, private
file, or brokered delivery bytes travel only through process-owned anonymous handles. The manifest
contains their checked references, destinations, descriptor identities, and lengths, never the
payload. Release closes those handles, revokes leases, removes private staging, shuts down every
proxy worker, and only then reports complete teardown.

Native cleanup kills and drains only the exact owned cgroup leaf. Recovery requires the exact C2
root birth identity and a leaf member whose live process ancestry reaches that root; mismatched or
inaccessible state remains fail-closed. The crate contains no unsafe Rust; Linux syscall wrappers
are provided by reviewed safe dependencies.
