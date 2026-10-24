# Red Team Phase 3 — real-interface campaign

Date: 2026-10-23.

Result: **Phase 3 passed its bounded contained-job and dependency-preparation campaign.** A
source-built Red Team module installed into disposable Peritus state and was operated through the
real TUI, daemon, native module host, process gateway, Linux sandbox helper, cgroup v2 hierarchy,
and retained module state. No provider inference was used by the local module commands.

This evidence covers the Phase 3 job, containment, lifecycle, and dependency-preparation
foundation only. It is not evidence of broad security investigation, source-build installation from
the market, patch generation, or a complete Red Team product journey. The published partial report
correctly kept all 15 security families unexamined and claimed no broad finding.

## Candidate identity

- Project Peritus base: `8933e81370a86b02009471eb517c1bc30bdd815f` on `develop`.
- Module Market base: `6b2cca73161b3f0e4ab004f9a124e4e01e84ca89` on `main`.
- Mirrored module-host contract SHA-256:
  `2f2a79f4b28d83dc1c23022fe93d804b98edc81bbc1e29f2f4ebd8558a65c363`.
- Phase 3 state fixture SHA-256:
  `1aae27bfa39977af2a6ffbbf27b30b1ceaab3bdc6622730ad1a88eac6942196c`.
- Release Red Team executable SHA-256:
  `5a262cc3846a12e8d3de85826f0fc69d1c6221c01d8c2b2f02f139a065b60ee0`.
- Disposable installed package digest:
  `2d115df08be4dc76d058e4f69eef4edba329c6cbfefdba463194e2a0d3422c42`.
- Release Peritus daemon SHA-256:
  `225f58f880db0b52c631d36be36dd9bcde123a878551eaa10c96c821d375da9e`.
- Release Linux sandbox helper SHA-256:
  `d94bf6ed74ba06ae1bfbf83c27c08efec702be154a9cb6676bd3747bd4bf85fa`.

## Real product path

Run `4349c756e91edce6481ad6a638ca0124` crossed the real terminal client and daemon. Its
final scope revision was `scope-2-4d0b34125b63d4ab`; its source snapshot and manifest artifact
were both `4d0b34125b63d4ab7ade1df2a514f99fa8c2cae2b98c5244016cd16048521efc`.
The generated report retained 13 host-owned jobs and 11 trusted terminal receipts.

The Linux proof adapter was `linux.canary-disclosure.v1` over backend `peritus-linux@1`.
Every proof receipt bound the selected source digest, execution-plan digest, independently generated
oracle digest, exact module digest, scope revision, source snapshot, terminal disposition, output
artifact identities, resource policy, network policy, and cleanup result. The preparation adapter was
`https.dependency-fetch.v1` over `peritus-host-https@1`.

## Observed containment and oracle behavior

- The actual vulnerable target disclosed the hidden host-generated canary and received
  `ViolationObserved`. The corresponding safe target received `PropertyHeldForCase`.
- A target that printed plausible success words without the hidden canary also received
  `PropertyHeldForCase`; target-authored text could not manufacture a trusted receipt.
- An isolated target observed `/etc/passwd` as blocked, an outbound connection as blocked, and both
  `HOME` and `GITHUB_TOKEN` as unset. Every proof receipt recorded `deny_all` network.
- Explicit cancellation of a target with a live descendant produced a `cancelled`, indeterminate
  receipt with complete process-tree cleanup.
- A busy loop exceeded the native deadline and produced a `timedout`, indeterminate receipt without
  relaunch. Memory allocation pressure failed inside the 128 MiB ceiling and was retained as
  indeterminate. A file writer hit the 64 MiB storage ceiling with exit code 153 and complete cleanup.
  A process fan-out was cancelled and its descendants were reaped.
- Revoking live source authority cancelled an active proof. Killing and restarting the daemon left
  the interrupted job `Indeterminate`; polling recovered the durable record and did not repeat the
  side effect.

## Dependency preparation boundary

The host fetched the pinned Rust 1.85.0 README from
`https://raw.githubusercontent.com/rust-lang/rust/1.85.0/README.md`, resolved and retained the public
IPv4 and IPv6 socket identities, accepted 3,138 bytes only after matching SHA-256
`0f0d7a250d44e4f69929d45a1128d7ec341a4291ffc0c0a9686344cc46d8dedf`, and recorded
`declared_https_public_only_then_disconnected`. No redirect occurred. Requests for `localhost` and
for a URL containing user information were rejected. Proof execution remained a separate deny-all,
credential-free path.

The machine-readable companion lists all 13 scenarios and the exact trusted receipts:
[red-team-phase3-evidence.json](red-team-phase3-evidence.json).

## Defects found and corrected during the campaign

1. Scoped-source target verification hashed an already finalized SHA-256 digest a second time, so
   every valid target was rejected. It now compares the encoded finalized digest directly.
2. Fresh private job workspaces omitted the protected metadata roots required by the Linux backend's
   anti-TOCTOU projection. They now create empty `.git`, `.peritus`, and `.crosslink` roots before
   the backend masks them and freezes the plan.
3. The fixed Linux seccomp allowlist omitted ordinary POSIX process/session identity calls required
   by the system shell. The bounded calls are now admitted inside the already isolated PID namespace
   and cgroup.
4. The first descendant replay fixture exited before cancellation because it invoked an unmounted
   `/bin/sh` alias. The accepted replay uses a subshell of the admitted interpreter and requires an
   actual `cancelled` receipt, preventing false cancellation evidence.

## Build and analysis record

The Red Team crate passed `cargo check`, strict package Clippy, and a release build. Project
Peritus's product runner, daemon, and external benchmark caller passed `cargo check`; release CLI,
daemon, TUI, and Linux sandbox-helper binaries built. Unit tests and formal verification were not
run.

Workspace-wide formatting remains obstructed by pre-existing deleted shell-tool fixture files under
`crates/tools/peritus-tools-shell/tests/fixtures/`. Workspace-wide strict Clippy remains obstructed
by the existing unrelated lint backlog; neither is represented as Phase 3 evidence.
