# I6 completion: shared full candidate observations

Status: **complete for phase seven (I6). All seven increment contracts, I0–I6, are complete.**
Architecture verdict: **ready for opt-in managed Linux use**. Separate capture remains the
default on every platform. This is bounded qualification of the efficiency design, not a
whole-codebase or all-platform certification.

## Behavior and compatibility

`local_context.candidate_observation = "shared"` derives the existing repository fingerprint
and source digest from the same complete managed preimages. The recorder then takes a second
fresh full capture and compares both identities. An accepted observation requires two complete
source enumerations instead of the original repository/source/repository sequence of three.
The existing three-attempt limit and explicit unstable-workspace error remain.

Every material tool, command, verification, review and settlement boundary still observes its
candidate. Source bytes, kinds, permissions, nested repositories, HEAD handling, existing path
filters, real-index preservation, execution dependencies and candidate qualification retain
their existing owners. No previous boundary, notification, model declaration or size/mtime
token replaces a full capture. Git object publication and synchronization remain real work.
In-place scopes retain their existing observation path and fresh final qualification.

The production amendment in this phase is limited to `execution/checkpoint.rs` and
`context_config.rs`: the recorder activates sharing only on Linux, and the policy documents
that fallback. The already implemented common full-capture projection is retained. Unsupported
platforms use the original path even if Shared is configured. Public enum/config spellings,
identity encodings, baseline files, checkpoint reader floors and durable continuations do not
change. Set `"separate"` and continue from current state to roll back; no evidence is discarded,
old checkpoint selected, command relaunched or user state restored.

Independent-read parallelism and schema packs were considered and left out of this delivery.
Serial plans and complete catalogs retain the demonstrated request/layout benefits without a
new scheduler or activation protocol. No additional net benefit has been demonstrated for those
optional experiments. Notification-assisted candidate reuse is also not introduced.

## Final-cohort real-interface qualification

All mutable state lives under `/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`.
The actual daemon/CLI/TUI build supplies the libraries used by the public consumers. Native
inference uses the official Codex runtime and `gpt-6.1-sol`; no provider or Git output is mocked.

| Campaign | Observed result |
| --- | --- |
| `i6-completion-freshness` | Eighteen paired identity groups preserve exact legacy/shared identities and independent source/index snapshots. They cover same-size/restored-mtime changes, root/file/nested modes, additions/deletions, symlinks, staged state, root/nested HEAD changes, committed/unborn nested repositories, toolchain and ignore configuration, and unchanged ignored-source scope. |
| Same campaign, additional cases | A tracked FIFO is rejected by both paths. A real linked worktree preserves its separate real index. Read-only owned Git object directories make new-blob publication fail under both policies; restoring permissions permits exact reacquisition. |
| Same campaign, concurrent writer | Twenty-four observations under an actual atomic file publisher return one of two independently preobserved identity pairs. All 24 are accepted exact pairs; no mixed identity or workspace temporary file appears and the real index is unchanged. This is bounded race coverage. |
| `i6-completion-interruption-r2` | An actual restored-mtime amendment between captures requires attempt two and returns the amended pair. Native consumer kills between captures and after the second capture produce no accepted reply; fresh processes reacquire the current exact pair. Invalid real HEAD metadata is rejected; repair permits fresh acquisition. No inference or task effect is dispatched. |
| `i6-completion-managed` | Both Separate and Shared complete actual managed design/write/CSV gates/independent review with Accepted settlement. Exact final CSV bytes, unchanged manifest, original HEAD/index, changed-path set and request/checkpoint bindings are checked. |
| `i6-completion-in-place` | Shared configured on an in-place run also reaches Accepted through the existing scope path, exact CSV gate and review. Full source/manifest/HEAD/index checks pass. |
| `i6-completion-resume/unchanged-rollback` | Cancel the real writer at Checking, retain its durable continuation, then start a fresh process with Separate. The unqualified candidate resumes at checks, requires current review, and reaches Accepted without a second writer turn. |
| `i6-completion-resume/source-amendment` | After the same interruption, amend CSV bytes with equal size and restored mtime. Fresh Shared continuation enters Designing/Writing, actually reads the distinct amended marker, repairs the exact requested CSV, and reacquires gates/review/obligations before Accepted. |
| `i6-completion-head-resume` | After interruption, independently make a real empty commit. Separate continuation observes unchanged source and changed repository identity, resumes at Checking without Writing, reacquires qualification, preserves the new HEAD and original index, and reaches Accepted. |
| `i6-completion-compiler`, `i6-completion-plans`, `i6-completion-varied` | Eight compiler groups, nine plan/compiler/older-v3-reader/rollback groups, and nine varied groups with 264 API actions pass on this cohort. |

The three interrupted candidates are explicitly unaccepted with missing current gate/review
evidence. The final published checkpoints have current gates, review and obligations. The
source amendment is verified against actual source-read receipts, not a final checkpoint
incorrectly treated as an initial observation. Existing I5 command ownership/control recovery
is unchanged; these capture-only interruptions neither launch nor reconcile a command.

Six final journeys reach Accepted, using 46 actual provider responses in total, including work
before the three deliberate interruptions. `i6-completion-native-audit.json` reconciles every
response with canonical C5 decoding, actual attempts and current request/view bindings. Full
input includes cached input; absent reasoning/native-total/billing counters remain unknown.
All six traces have zero provider retry records. The two full managed journeys report:

| Same task/model, fresh local state | Separate | Shared |
| --- | ---: | ---: |
| Provider requests | 7 | 7 |
| Full input tokens | 143,456 | 143,065 |
| Cached-input subset | 17,920 | 0 |
| Output tokens | 930 | 1,116 |
| Completion seconds | 71.226 | 75.200 |
| Final settlement | Accepted | Accepted |

These native journeys overlapped other qualification. Their small token difference and mixed
latency do not demonstrate a product speedup or token reduction from capture sharing. This
increment removes local duplicate observations; it does not change model capability, prompts,
tools, role count or acceptance requirements. Earlier increments' token savings are recorded
in their own reports and are not multiplied together here.

## Controlled local cost

The final warm comparison uses seed 192805, 256 independently generated 64-KiB files (16 MiB),
twelve pairs with alternating order, and symmetric native Git tracing. Both paths are warmed
first. No build, provider journey or other campaign of this task overlaps these timed samples.
Every pair has equal identities and independent byte/index preservation.

| Complete observation | Separate | Shared |
| --- | ---: | ---: |
| Actual complete source enumerations | 3 | 2 |
| Samples | 12 | 12 |
| Median seconds | 0.127375 | 0.087773 |
| Mean seconds | 0.127268 | 0.087994 |

Median local capture time falls **31.1%**, with one-third fewer full enumerations. Git objects
and OS caches are warm. This is not a cold-disk, every-filesystem or end-to-end latency claim.
Shared remains opt-in because the local gain does not establish a general product latency win.

## Build, platform checks and reproduction

The final source passes:

```sh
cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
cargo clippy --locked --jobs 2 -p peritus-workspace -p peritus-product-runner \
  -p peritus-daemon --all-targets --all-features -- -D warnings
cargo check --locked --workspace --all-targets --all-features --jobs 2
cargo check --locked --jobs 2 -p peritus-product-runner --all-features \
  --target x86_64-pc-windows-gnu
cargo fmt --all --check
git diff --check
```

Windows cross-compilation passes with existing unrelated platform warnings; native Windows
behavior is not qualified. macOS cross-compilation was attempted with the installed Rust target
and Clang, but bundled SQLite cannot compile against Linux headers without an Apple SDK
(`i6-completion-macos-check.log`). No macOS pass is claimed. Both platforms retain Separate
capture. No GUI journey or new platform-specific hashing implementation is included.

See [the replay commands](reproducers/incremental-context/README.md#i6-full-candidate-observations).
Build/check logs, Git traces, exact inputs, raw native traces and independent oracles use the
`i6-completion-*` prefix. The source/binary inventory is
`i6-completion-source-identity.json`; it binds the dirty tree and actual binaries, not coverage.
The ordinary ProductRunner consumer is
`96311d50d9849e3b74e5832eac7771d55c3e19f5f3d6147e71b052d2f37c37ab` (SHA-256); the full
capture consumer is `c000fe71b1d3e67fecc7304cebb85a6c44f17a194474df82e9a572f6862a514c`.

The retained first interruption attempt failed its fixture's equal-size assertion before
exercising the program. The first continuation driver incorrectly expected a published initial
checkpoint from ordinary phase events, which carry none. Its actual native source-amendment
run completed Accepted; `corrected-result.json` records independent trace revalidation and the
driver now checks actual amended reads. Neither case changed a production gate or weakened
the expected freshness outcome. Failed setup/oracle attempts are not passing samples.

Source coverage follows policy decoding/defaults, recorder construction, managed/scoped branch
selection, complete capture and both digest projections, root/nested source and HEAD handling,
candidate sequence/evidence reconciliation, durable continuation and current gate/review/final
settlement callers. Runtime source/index observations are independently checked. Inventory
counts and compilation do not stand in for this scoped caller review or behavioral evidence.

Branch `develop`, HEAD `d58ccbed426fba0a361a6533bb4ddb80726ba2ce` and unrelated work are
preserved. No commit, push, release, deployment or live user configuration change is performed.
