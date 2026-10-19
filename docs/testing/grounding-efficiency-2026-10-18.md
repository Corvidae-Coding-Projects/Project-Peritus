# Focused grounding and context reuse

Follow-up: [one startup grounding check](single-grounding-2026-10-18.md) supersedes the startup
phase behavior described here. Listing a nonempty root now completes that phase without requiring
an additional file read. Later directory changes invalidate observations of emptiness, not startup
completion. Target-specific hash and permission checks remain separate. The results below record
the earlier campaign rather than the final one-check handoff.

The human coding path now reuses host-verified source observations and exact accepted task designs. New roles and recovered context do not mandate another repository inventory. This changes discovery and prompt delivery, while preserving tool authority, independent review, and the developer loop.

## Behavior and ownership

- The product tool executor stores run/workspace/root-bound source receipts beside the host-owned trace. It rechecks current read policy and full SHA-256 contents before crediting an old observation. Hash checks perform local I/O; they do not resend full source bodies to the model. Missing, changed, deleted, escaped, or newly forbidden paths cannot grant reusable edit credit. An existing target changed after a read must be read again before mutation.
- Exact task/context/design receipts skip a duplicate designer provider pass. A ready guided design can enter implementation directly only through the explicit saved-design action and the immediately following input generation. Task steering, context changes, or draft changes invalidate stale reuse. This is reuse of a task contract, not transfer of implementation permission or acceptance.
- Designer, writer, fixer, and reviewer prompts carry a short common policy and role instructions. Full engineering, real-testing, computer, architecture, implementation, review, and Rust resources remain available through `skill_read`. Current user restrictions still govern commands and checks.
- Deterministic context assembly keeps recent complete exchanges and prioritizes hash-current task source and skill evidence under view pressure. Identical inspection exchanges/previews are collapsed without replaying effects or deleting the original archive. Current permissions are reapplied to automatically reused source evidence. Optional auxiliary semantic compaction requires actual context pressure and new evidence.
- Host traces emit content-free measurements in `.efficiency.jsonl`. These report rendered read bytes and identical repeated pages, local receipt-revalidation I/O, accepted design reuse, actual provider input/cache usage, system policy size, per-view context savings, and the first changed workspace file or completed declared external operation. Time to useful action uses monotonic attempt time; wall time is separately labeled because it includes pauses. Unknown provider usage remains null in per-response records. Cumulative accounting snapshots must not be summed. The receipt I/O counter covers cache restoration, not additional pre/post-read or archive-preview hash checks.

## Replay

Build the ordinary public API consumer outside Cargo workspace membership:

```sh
python3 docs/testing/reproducers/build-grounding-efficiency.py target/grounding-efficiency/consumer
python3 docs/testing/reproducers/grounding-efficiency.py --consumer target/grounding-efficiency/consumer/consumer --output target/grounding-efficiency/replay
python3 docs/testing/reproducers/summarize-grounding-efficiency.py /absolute/path/to/run.trace
# Authenticated live concurrent-edit replay; output must be a new disposable directory:
python3 docs/testing/reproducers/grounding-concurrent-edit.py --consumer target/grounding-efficiency/consumer/consumer --output target/grounding-efficiency/concurrent-new
```

The fixture driver uses actual workspace tools, actual durable context storage, and independent filesystem/hash/content oracles. Its context-pressure case supplies real tool observations through the public context port; it does not pretend that a provider selected those calls. Separate `live` consumer runs exercise the real authenticated provider and full product pipeline on disposable applications. No unit, mock-provider, formal, or qualification suite is used.

## Defects found during this change

| Defect | Correction | Replay |
| --- | --- | --- |
| Tool descriptions still demanded a fresh inventory every invocation. | Align catalog descriptions and writer/reviewer wrapper prompts with current or host-verified evidence. | Inspect the actual delivered tool schemas and live role behavior. |
| A duplicate archived read could be reintroduced beside its retained exchange. | Seed canonical page deduplication from actual retained tool results; ignore changing memory receipts. | Repeated exact pages under context pressure, including distinct source receipts. |
| A retained skill page disappeared when its full preview exceeded the remaining token target. | Fit a labeled excerpt to actual remaining context space, keeping the exact full-source handle. | Tight pressure retains the task fact and real-testing title once. |
| Empty-root credit survived a later nonempty root listing. | Replace, rather than accumulate, root-empty status at each current listing. | Empty → create file → nonempty list → fresh process. |
| A ready draft could bypass design after intervening task changes. | Require the explicit implementation action and the immediately subsequent generation. | Fresh and stale guided-design handoffs. |
| Reused source previews did not consult live read restrictions. | Reapply current capabilities and the workspace access policy before replaying source evidence. | Recover context after path protection and Read revocation. |
| Status/no-op commands could end the useful-action timer. | Count observed workspace changes or completed declared external effects. | Inspect timing event against actual first edit and preceding tool calls. |
| Reviewer lacked the saved collaborative draft and reported its selected feature as unrequested. | Deliver the exact host-loaded draft, version, stage, and input generation to independent review as well as writer/designer. | Full ready-design coding run with concurrent source edit. |

## Results

- Main host/context campaign: **26 scenarios passed**. Covered fresh-process reuse, full independent SHA-256 checks, same-size/same-mtime changes, deletion/recreation, symlink replacement, private/read permission revocation, scope/root changes, read-only enforcement, all seven guidance resources, empty-to-nonempty roots, and source/skill retention under an 18,000-token context-profile fixture.
- Independent campaign: **20 named checks, 56 fresh consumer invocations passed**, including generated nested/space/Unicode paths and 0/1/4096-byte bodies with original mtimes restored after changes. Task facts remained once in each pressure view; the real-testing title remained once in every view after its read. Its final estimated view was **15,264 tokens**, with **62,432 estimated tokens displaced** from that view. Twenty-seven local view records matched twenty-seven actual published checkpoint generations; these were not twenty-seven auxiliary provider compactions.
- The independent pending-action handoff reached Writing with **zero provider requests and zero tool calls**, preserving the exact selected saved brief. A separate real-provider run with intervening steering rejected stale ready-draft reuse and performed a new design pass, despite the ready-action hint being true.
- A real authenticated `gpt-6.1-sol` run built a tiny existing Python CLI feature, ran actual commands, passed independent review, and settled **Accepted**. The external oracle checked four exact stdout/exit cases and preserved an unrelated file. A same-task restart used its host design receipt and spent **zero requests redesigning**; its first changed file occurred **14.95 seconds** into that attempt. The initial design had used four requests. This is a single feature replay, not a general timing benchmark.
- That first complete run recorded **1,264 rendered read bytes**, **172 identical repeated bytes**, and **595 local receipt-revalidation bytes**. Across the first design and reused implementation, provider-reported input totaled **227,181 tokens over 12 responses**, with **0 reported cached input tokens**. The restarted implementation itself used eight requests and 163,881 input tokens. Snapshots were replaced by response identity rather than summed twice. The workflow emitted **zero auxiliary compaction calls** below pressure.
- Conservative prompt comparison: the old shared workflow text alone was **31,569 bytes**; the largest complete system policy in that actual coding run was **11,301 bytes** (**64.2% smaller**). The old denominator excludes its other embedded skills. This does not claim a paired billing or wall-time speedup.

The corrected concurrent-edit replay settled **Accepted**. An independent process appended a source comment after the actual read receipt; the host rejected the stale patch, the agent reread the changed source, and the final program preserved the comment and unrelated file while passing exact CLI output checks. The fresh reviewer accepted the saved-design task after its missing host-draft handoff was fixed. This run used **8 provider requests**, **521 rendered read bytes**, **0 identical repeated bytes**, **162366 provider input tokens**, and **8064 provider-reported cached input tokens**. No extra designer request or auxiliary compaction was made. The final release-mode launcher/CLI, daemon, and TUI build succeeded with two compiler jobs under a 6 GiB cgroup ceiling. CLI/TUI help and CLI/daemon version entrypoints returned success. Daemon `--help` returned its usage response (exit 2); no live daemon was launched or replaced. Private traces, complete provider observations, original failures, receipts, and generated fixtures remain under `target/grounding-efficiency/`; the Markdown records defects and scope without importing full traces into source control. The initial concurrent-edit replay was deliberately stopped after reproducing the missing saved-draft reviewer handoff; it is not counted as a passing completion.

These are bounded coding-path and context tests. They do not certify the entire application, all providers, GUI/TUI actions, or multi-hour workloads. The pending-action daemon capture was inspected in source; the independent executable drove the public ProductRunner boundary, not the daemon transport. Existing user daemons, settings, and installed binaries were not changed.

Final source replay: the final consumer passed all 26 host/context scenarios again and the pending ready-design handoff again used zero provider requests. The final source-only wording adjustment allows unchanged hash-verified evidence when repeating conserved findings; the preceding full provider replay had no conserved findings after its corrected first review.
