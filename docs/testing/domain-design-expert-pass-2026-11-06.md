# Expert design pass qualification — 2026-11-06

For the subsequent actual browser run and document-gate correction, see
[browser-design-update-live-2026-11-06.md](browser-design-update-live-2026-11-06.md).
The installation identities below describe the initial rollout snapshot.

This implements [.design/domain-design-expert-pass-remediation.md](../../.design/domain-design-expert-pass-remediation.md)
on develop. The earlier steering-only repair is historical evidence, not qualification of this workflow.

## Behavior and ownership

- `design/source.rs` captures the actual supplied workspace design together with current user
  instructions, preserving exact bytes, separate origin metadata, immutable history, and schema-1
  readers. Selection verifies scope, regular-file identity, current digest, and read permission.
- `design/expert_pass.rs` owns durable questions, expert-authored amendments, compact current
  state, exact rendering, and host-assigned evidence/obligation/acceptance/change links.
- `design/expert_tools.rs` provides focused worklists, amendment saves, exact source selection,
  bounded readers, unapproved recovery, small atomic patches, and publish-by-identity. Every
  mutation binds the current source SHA-256 and predecessor version. Ready passes are immutable.
- Existing expansion validators still establish all required checkpoints, current evidence,
  source preservation, material-question behavior, artifact identities, and Ready eligibility.
  Importing an old proposal does not validate its conclusions or evidence. A malformed range can
  be repaired without regenerating the draft; it cannot be published before correction.
- `design.rs` and the guided conversation callers use the same expert tools and full domain
  policy. Automatic coding authority and guided Design's separate implementation boundary remain
  distinct. Source bodies enter user/tool data, while current host metadata replaces system guidance.
- Local context binds both storage and trace identity to the source and active question. Prior
  work stays in exact private artifacts rather than automatically replaying old observation loops.
- The agent prepares policy before selecting required tools and admitting requests. Progress
  accounting counts actual admitted requests and informed completed response batches. Parallel
  rejected calls do not consume several response opportunities.

## Reproductions and regressions

The native runner suite passes 77 tests. New filesystem-backed cases cover amendment publication,
idempotent retry, stale version and source rejection, restart and steering, malformed evidence repair,
source-file changes, symlinks, denied permission, unavailable primary evidence, useful partial work,
exact context lineage identities, and progress scheduling with parallel calls.

A retained candidate larger than 100 KB is corrected with a request smaller than 512 bytes. Exact
unaffected Markdown survives, failed patches leave the saved bytes unchanged, and publication uses
the retained candidate. This is a native state replay, not a provider efficiency measurement.

The browser-sized qualification copies the actual Jackdaw working source and its 43,008-byte
design into a disposable repository. It requests a bounded expert pass on worker reliability and
Fetch semantics. Independent checks require actionable evidenced amendments, authoritative web
evidence, a published self-contained design, preserved original design bytes, and an actual Rust
source edit after handoff. The harness cancels after independently observing that edit; it does
not claim browser implementation, test completion, independent review, or release qualification.

An early real replay exposed shared trace identities despite separate context directories. The
corrected binding and a permanent regression prevent one question's observations from being
recovered as another question's proposals. Another replay appended to the disposable input design;
the qualification request was clarified to require byte preservation and a separate expanded
artifact, then replayed successfully. These failed attempts are not counted as passing coverage.

## Credentialed results

Provider: the existing authenticated OpenAI Codex runtime, `gpt-6.1-sol`, Linux. No user model or
effort settings were changed. Other providers and platforms are not qualified by these runs.

The first passing browser replay published two researched amendments and performed the exact
authorized Rust edit in 22 model requests, using 1,227,397 input and 13,261 output tokens. Evidence:
`~/.cache/peritus-expert-browser-bound-20261106/browser/`.

The source-bound recovery replay imported those amendments, changed only the worker amendment,
preserved the other amendment exactly, and published at input revision 2 in eight requests using
345,544 input and 4,502 output tokens. It reused retained evidence without restarting online
research. Evidence: `~/.cache/peritus-expert-recovery-final-20261106/browser-recovery/`. Its original
summary's provider-attempt count includes the copied earlier trace; use the eight-request count or
filter the appended trace segment for attempt timing. The harness now measures that segment.

The final source-bound fresh replay also passed: two amendments and the actual Rust edit in 24
requests, 1,393,659 input and 14,610 output tokens. The first useful amendment was saved after
227 seconds and the second after 476 seconds. There were zero legacy state-tool observations or
full-ledger submissions. The model supplied 30,800 bytes of new amendment arguments and a
1,857-byte publication request. Explicit source/draft paging remained available and was used;
these bytes are not free and the design still costs model context. Evidence:
`~/.cache/peritus-expert-qualified-20261106/browser/`.

The recovery-only trace has one 105-byte import request, five bounded page reads, one 11,429-byte
amendment replacement, and one 1,647-byte publication request. It made zero new web calls.
The unrelated amendment remained byte-identical; the changed amendment was replaced as a unit.

Guided Design also published a ready mechanical plan in eight real provider requests, preserved
the original Rust source, and never entered Writing. Evidence:
`~/.cache/peritus-expert-interactive-20261106/interactive/`.

These are bounded scenarios, not paired latency benchmarks or proof of exhaustive domain review.
Legacy full-proposal repair is covered by the native large-candidate replay; live recovery covers
the saved expert-pass route. Material answers, attachments, corruption and source boundary cases
retain their existing native coverage; they were not all rerun through every provider or UI.

Strict Clippy for the runner and agent (all targets/features), the full workspace check (all
targets/features), formatting, and the 77 runner tests passed. The agent package's default test
target contains no tests; its changed execution path is exercised by the real provider journeys.
All commands use locked dependencies with two build jobs. No provider credentials are retained
in this report or fixture summaries.

## Local rollout

Built the release binaries and atomically installed `peritus`, `peritusd`, and `peritus-tui` for
doll and builder. Each account's original binaries and before/after manifest are retained under
`~/.local/state/peritus/install-backups/expert-pass-20261106T152822Z/`.

| Binary | Installed SHA-256 (both accounts) |
| --- | --- |
| `peritus` | `580ec9efe8b0ff8ef298d9e4f31439a0389d0a91ef40e5b301b492c09c13b225` |
| `peritusd` | `3d508ca0a379a9c122182e83823ba9d92701fe231ee023c55e77fe0a5b1e3489` |
| `peritus-tui` | `d313aee61bfe43e7da78ac499157049797002f0c4028b3e1d07930c8a09d4298` |

Orderly shutdown completed cleanly for doll. Normal launcher/TUI startup started each account's
new daemon; both independently reported `ready-read-write`, `mutation_ready=true`, and no
diagnostic. Both running daemon executable digests match the installed release build. The two
verification TUI clients exited normally; daemons remain available. Provider setup and selected
models were preserved.

The original Jackdaw run was independently observed as Cancelled before and after rollout. Its
existing workspace edits are preserved; canceled work was not resumed. No commit, push, release,
or update to the actual Jackdaw checkout was performed by this expert-pass implementation.
