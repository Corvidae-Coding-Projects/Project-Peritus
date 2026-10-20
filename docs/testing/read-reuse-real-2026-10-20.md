# Workspace evidence reuse — real testing, 2026-10-20

## Outcome and scope

Implemented on `develop`, based on `cfaddddb7548a63c47329d5386fd99ad5b6bda09`.
Unchanged workspace document ranges already present in the exact model view return successful
reuse receipts. Omitted pages restore from the existing durable role archive. Overlapping reads
deliver missing line/byte intervals with accurate continuations. Changes refresh the target,
and independent review obtains its own evidence. No read-count, token, or run-size limit was added.

The existing grounding receipts still own completion of startup grounding. Workspace tools retain
permissions, protected-path checks, argument admission, current OS access, content-hash validation,
and grounding bookkeeping. Reuse planning runs before dispatch and never grants authority. The
archive remains the durable source of truth; there is no parallel persistence format to migrate.
Pre-change observations lacking exact range metadata refresh conservatively. Unavailable derived
reuse data falls back to an ordinary authorized read.

Context reconstruction also excludes already-selected overlapping source bytes. Truncated archive
previews remain valid structured pages with their exact displayed coverage. Plans with changed
source bindings remain explicitly stale, non-authoritative progress; current user requirements
still govern. A compact host receipt identifies the last completed changed workspace operation
without claiming the entire task is complete. Tool help and memory policy explain these behaviors.

## Actual interfaces and results

The final replay uses `MemoryTools`, `WorkspaceDeveloperTools`, `LocalContextHandle`, context
assembly/checkpointing, and fresh-process recovery through their public production APIs.
The fixture owner independently generates files, expected content, edits, permissions and source
token/byte coverage. No substitute reader, simulated provider response or unit-test framework is used.

**20 scenarios passed**, including **48 seeded generated range requests** (seed `20261020`).
The campaign performed 154 durable workspace reads and one context update, plus two direct base
executor reads for comparison. Seven read proposals intentionally exercised ordinary rejection.

| Required behavior | Observed result |
| --- | --- |
| Exact reads and contained ranges | Successful body-free reuse, including 20 consecutive deliberate repeat requests. |
| More than 16 observations, unrelated edits | Original current evidence remains reusable. |
| Adjacent and disjoint ranges | Only missing intervals delivered; continuations skip visible intervals. |
| Omitted evidence and fresh process | Original pages restore, then subsequent requests reuse the current delivery. |
| Archive reconstruction | Rendered pages count as visible; overlapping source text appears once. |
| Partial archived preview | Only displayed coverage credited; omitted suffix restored. |
| Truncated Unicode lines | Exact byte coverage, UTF-8 boundaries and response budgets preserved. |
| Initial read midway through a line | Missing prefix and suffix filled without resending the visible middle. |
| Empty, newline-only and EOF inputs | Correct content/EOF behavior and reuse. |
| Same-length edit with restored mtime; file replacement | Stale evidence refreshed for the original requested range. |
| Current permissions metadata | Restored content reports current file mode. |
| Read permission revoked; protected path; symlink; malformed calls | Normal denial preserved without cached content disclosure. |
| Independent reviewer | Own initial observation required; its subsequent delivery is reusable. |
| Changed source underpinning a retained plan | Plan remains visible as explicitly stale historical progress. |

Applied read outcomes in the final API replay:

| Outcome | Requests | Delivered content bytes |
| --- | ---: | ---: |
| Fresh read | 40 | 103,494 |
| Already visible | 65 | **0** |
| Read missing spans | 29 | 8,565 |
| Restore omitted archive evidence | 13 | 102,728 |
| Intentional rejection | 7 | 0 |

The direct base-executor comparison returned the same 1,030-byte content twice. The decorated
path returned no content for its repeated visible range. This comparison exercises the current
base executor without the decorator; it is not a separate old-commit performance benchmark.
Restored bytes are deliberately delivered when their evidence is absent from the current view.
Candidate and applied reuse events are distinguished, with actual returned body/response sizes.

## Live official-provider runs

Two disposable ProductRunner runs used the installed official Codex runtime and `gpt-6.1-sol`.
Both implemented a small Python greeting CLI through normal design, writer and reviewer paths,
operated its actual CLI, and reached `Accepted`. No unit tests were created or run.

- Normal task: eight provider requests, eight tools; first changed file at 44.1 seconds.
- Deliberate duplicate-read diagnostic: ten provider requests, nine tools. The writer requested
  the same design range twice, received `already_visible` with **zero content bytes** and a
  319-byte response, then wrote the program about 20.6 seconds later and completed it. It did not
  require a third copy of that source body.

Both programs were independently invoked from fresh CLI processes for `Raven`, `--upper 猫`,
and missing name: exact greeting output/exit 0 and usage on stderr/exit 2 all matched the prewritten
requirements. These live runs used the same reuse implementation before the final tool-help wording
addition; the final rebuilt dependency cohort passed the full API replay afterward.

## Failures found during implementation

The replay found and retained two distinct failures in early builds:

1. Adding reuse metadata could exceed a small caller-selected response budget. The final fitter
   includes metadata and slices exact UTF-8 content with logarithmic fitting passes.
2. Accumulating all prior source handles in continuation receipts eventually crowded out the body
   and caused a fallback read of already-visible bytes. Missing-span receipts now carry a compact
   reused-page count; pure reuse receipts trim reference lists to fit. The middle-of-line replay
   checks every returned byte against separately generated source bytes and rejects overlap.

Concrete original outputs remain outside the checkout in `campaign-1` and `campaign-4` under the
artifact root below. Both saved workflows pass in the final replay. An initial plan fixture lacking
a required supporting source was corrected as a driver error; that was not a product defect.

## Reproduction and build

See [replay instructions](reproducers/read-reuse/README.md), [native consumer](reproducers/read-reuse/consumer.rs),
[independent fixture driver](reproducers/read-reuse/replay.py), and [compact observed results](reproducers/read-reuse/observed-results.json).

Artifact root: `/home/doll/.cache/peritus-review-20261020/`.

- Final native build: `read-reuse-final-build.jsonl` / `read-reuse-final-build.log`.
- Final API campaign: `campaign-6/results.json`, `campaign-6/operations.jsonl`, and isolated state.
- Live task: `live-read-reuse/`; deliberate repeat: `live-duplicate-reuse/`.
- Independent CLI readback: `independent-cli-results.json` in each live-run directory.

Build command:

```sh
systemd-run --user --scope --quiet -p MemoryHigh=1536M -p MemoryMax=2560M \
  nice -n 10 env CARGO_TARGET_DIR=/home/doll/.cache/peritus-tui-feedback-target \
  CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked --offline --jobs 1 \
  -p peritus-cli -p peritus-daemon -p peritus-provider-openai --message-format=json
```

CLI and daemon both built successfully and reported version `0.1.4`. Live runs had separate bounded
memory scopes and disposable files; no production daemon, installed binary, or user workspace was
changed. No unit, lint, formal, repository verification or qualification suite was run.

## Limits

This campaign covers workspace document reads with local memory enabled on Linux. Directory listings,
searches, skills, external references and attachments retain their existing behavior. Partial raw
`context_read` JSON fragments receive conservative visibility credit; a workspace read can restore
the exact page instead. Independent fresh review/settlement and current access checks remain intact.
No claim is made about every provider, native platform or TUI rendering path. The host mechanically
prevents redundant visible document delivery; a model can still choose unnecessary new questions.
The live diagnostic establishes that the observed provider used a reuse receipt and proceeded.
