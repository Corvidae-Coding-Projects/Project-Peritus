# Red Team Phase 2 — real-interface campaign

Date: 2026-10-23.

Result: **Phase 2 passed its bounded source-and-evidence acceptance campaign.** The host now binds a
campaign to the explicitly opened source root, publishes immutable content-addressed source and
manifest artifacts, issues revocable opaque handles, supports paged manifests and bounded reads,
recovers interrupted publication, reuses verified unchanged bytes, and invalidates only observations
that depend on changed or removed paths. The Rust module retains schema-two campaign state and
publishes honest partial Markdown and JSON reports. It executed no target code and claimed no
security finding.

This is evidence for the Phase 2 foundation only. Contained target execution belongs to Phase 3;
source-build installation belongs to Phase 4; investigation, patching, and the complete campaign
journey remain later work.

## Candidate identity

- Project Peritus base: `8933e81370a86b02009471eb517c1bc30bdd815f` on `develop`.
- Module Market base: `6b2cca73161b3f0e4ab004f9a124e4e01e84ca89` on `main`.
- Mirrored module-host contract SHA-256:
  `799905d01c48760230ac45bc32cb5960c7d41b34c6bb660a593f95ae800ce891`.
- Phase 2 fixture SHA-256:
  `52787ad320cef6512a34ef946e7882d69d136e7d74ba9ea17c530670d54500ff`.
- Release Red Team executable SHA-256:
  `da47f9b02898e56a09635802e746d326e15f20ccb988f39553984a95e240b0a8`.
- Disposable installed package digest:
  `23e1986b4b0c190c63e9656e136b729ff6ebbf4ef1dc14bee0b7c3d767014e0f`.

## Real product path

The primary run, `53ef6fe64abec64b5c5aa8cbf31c3e1c`, crossed the opened Chrome WebUI, Rust web
gateway, daemon `ProductRunService`, product runner, native module host, and the source-built Red Team
executable. Its final source revision was `scope-6-aab3f1a62244e3cb`; its snapshot and manifest
artifact were both
`aab3f1a62244e3cb2b315485d4dc17accb8189b2871c5ae63dbb753d29c40d39`.
The final manifest contained four files and 183 bytes, with `.git` and the user-selected `excluded`
path omitted. An unchanged refresh reused all four files and all 183 bytes without rereading their
source content.

The final checkpoint and both report formats showed two current observations, two invalidated
observations, all 15 security families unexamined, and zero claimed findings. That distinction is
deliberate: a captured or even observed source path is not evidence that the application is secure.

## Observed boundary and recovery behavior

- Captured tracked and untracked source from the explicit original source root while leaving the
  original project unchanged and executing no target files, hooks, generators, or configuration.
- Reopened the exact snapshot and evidence in a fresh process; repeated identical reads reused their
  retained artifact identity.
- Rejected excluded paths, `..` traversal, unsafe exclusions, an external symlink, a FIFO, a source
  root overlapping module-private state, out-of-range reads, zero-length requested capacity, and
  requests above the 1 MiB range limit.
- Independently recomputed ordinary-file SHA-256 values and matched the manifest.
- Retrieved a 109-file manifest across two browser responses and handled a path containing spaces.
- Correctly returned an empty EOF slice and captured/read a zero-byte file using the SHA-256 identity
  of empty bytes.
- Changed, removed, and unrelated files invalidated only dependent observations. A subsequent fully
  unchanged refresh reused the entire final snapshot.
- Revoked a scope and observed all later access fail through that handle.
- Killed the daemon during capture of a 1 GiB sparse file after partial staging. No scope became
  visible; `/module resume` reissued the exact retained host request, and recovery removed only the
  private regular staging file before completing a new capture.
- Mutated a 512 MiB source during capture. The broker reported `mixed_source_revision` and published
  neither a scope index nor staged result.
- Damaged a copied content artifact. Run `506dcf2d600050c9c7618c1ec805fc96` quarantined it,
  admitted no incomplete artifact, marked the failure non-retryable, and directed the operator to
  preserve the campaign for diagnosis and start a fresh campaign.

The machine-readable companion record lists all 24 scenarios and their exact identities:
[red-team-phase2-evidence.json](red-team-phase2-evidence.json).

## Defects found and corrected during the campaign

1. Managed-worktree runs captured the managed checkout rather than the explicit original source,
   omitting untracked input. The daemon-to-runner contract now carries a separate source root.
2. Killing capture could leave private `.tmp` staging that blocked later work. Recovery now removes
   only recognized regular staging files and fails closed on unexpected entries.
3. A retained host request could deadlock after a crash because `resume` did not reissue it. The
   module now persists the exact operation inputs, including manifest paging, and reissues them.
4. The content store rejected legal empty artifacts because the declared write limit was zero. Empty
   ranges and zero-byte source now use a nonzero transport allowance while retaining exact expected
   size and digest checks.
5. Corrupt-store errors were fail-closed but told operators too little. They now distinguish corrupt
   durable state, discourage retrying that handle, and give a concrete preservation/new-campaign
   recovery action.

## Build and analysis record

The Red Team crate passed offline `cargo check`, strict package Clippy, and a release build. Project
Peritus's product runner and daemon passed `cargo check`; CLI, daemon, TUI, and WebUI debug binaries
built, and the daemon was rebuilt after the last diagnostic-only change. Unit tests and formal
verification were not run.

Workspace-wide formatting remains obstructed by pre-existing deleted fixture files under
`crates/tools/peritus-tools-shell/tests/fixtures/`. Workspace-wide strict Clippy remains obstructed
by the existing unrelated lint backlog; neither was represented as Phase 2 evidence.
