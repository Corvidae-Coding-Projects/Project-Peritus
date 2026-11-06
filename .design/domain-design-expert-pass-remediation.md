# Feature: Productive domain-aware design passes

Status: implemented on develop; qualified with native replays and real fresh/recovered expert passes.
Date: 2026-11-06.
Branch inspected: develop, including the uncommitted design-steering repair.

Implementation and local rollout evidence:
[domain-design-expert-pass-2026-11-06.md](../docs/testing/domain-design-expert-pass-2026-11-06.md).
All four delivery stages below are implemented and qualified within that report's stated coverage.
The repaired binaries are installed for doll and builder; the user's canceled run is preserved.

## Summary

Make design enhancement perform the sequence demonstrated on Jackdaw: read the supplied design
and relevant current code, identify concrete domain questions, research authoritative answers,
record useful amendments and acceptance criteria, then publish and hand off. State inspection is
diagnostic support. A state observation, repeated read, or retry is not a design improvement.

Preserve the complete long-form domain policy, elevated instruction priority, all original product
requirements, exact source preservation, evidence validation, material-question rules, permissions,
recovery, and implementation/review binding. The fix changes how the expert does the work and how
the host records it. It does not require the user to choose an expert mode or another provider.

## User-visible behavior

For an existing design, the first useful output is an inspectable amendment stating the current
gap, its authoritative or repository basis, the proposed change, affected owners, and observable
acceptance. Save amendments as evidence becomes sufficient rather than waiting for one enormous
submission. Show progress such as "Researching redirect credential rules" or "Recorded worker
isolation amendment". Do not present archive reads or unchanged ledger observations as completion.

Publish a separate, self-contained expanded design after required questions are resolved. An
authorized implementation request proceeds to the writer automatically. Design-only work retains
its existing boundary and never starts implementation. A critical unresolved question produces a
useful partial design and an exact blocker. Necessary further research remains possible.

## Observed causes

1. **Bookkeeping dominates the expert interface.** `design/research.rs` requires complete nested
   records for every transition and complete Markdown for publication. The recovered Jackdaw
   submission is 97,680 bytes: 44,119 bytes of Markdown, eight domains, 28 evidence records, 21
   obligations, 25 acceptance mappings, and 21 change entries. Most identity/link fields and every
   already retained paragraph are repeated rather than new expert work.
2. **Observation is unusually expensive.** `observe` returns the ledger, the first source page,
   and the full retained proposal. The normal workflow gives the model a reason to request this
   material repeatedly instead of an explicit current question and useful next action.
3. **The supplied document is not consistently the source contract.** Automatic capture chooses
   direct user text or a guided draft. The workspace-file capture implementation exists, but the
   automatic path has no corresponding source-selection tool. In the observed run, the source was
   the user's instruction with appended steering, while `design.md` was separate repository
   evidence. That is weaker input modeling than the promised supplied-document workflow.
4. **Historical context grows with repeated bookkeeping.** The decision index deduplicates exact
   successful workspace and skill inspections, but not design-state observations or web reads.
   It conservatively retains other distinct observations. Design has a run-scoped context lineage;
   retrying does not establish a fresh, explicitly selected expert-pass context. These mechanisms
   explain potential duplication; their exact contribution to latency must be measured.
5. **The new guard is scheduled too late.** `execution.rs` selects required tools and guidance
   before `provider_turn.rs` calls `observe_model_context` on the built request. The design
   executor currently changes its checkpoint requirement in that later callback. It also counts
   individual calls as failed checkpoint attempts. Parallel calls can therefore exhaust the
   allowance without two separate informed opportunities to make progress.
6. **Qualification missed the representative workload.** The successful live steering case used
   a tiny mechanical function design. It demonstrates binding recovery, not an external-research
   enhancement of a substantial existing browser design. A stopped loop is not a successful pass.

These findings refine the earlier repair: revision refresh and draft retention are useful, but
they do not establish productive domain-expert behavior.

## Domain obligations and research basis

Applicable disciplines are agent workflow design, tool/API usability, state-machine correctness,
context selection, transactional publication, evidence provenance, and behavioral evaluation.

The expert owns judgment: applicability, missing obligations, interpretations, amendments, failure
behavior, and acceptance. The host owns known identities, revisions, digests, durable storage,
cross-reference construction, admission, and publication. OpenAI's function-calling guidance
specifically recommends moving known arguments and deterministic work into code and making tool
interfaces straightforward to use:
https://developers.openai.com/api/docs/guides/function-calling#best-practices-for-defining-functions

Verification must assess both the output and the steps taken to obtain it, including repetitive
commands and token growth. This is consistent with the official skill-evaluation guidance:
https://developers.openai.com/blog/eval-skills

These are applicable design principles, not proof that a proposed implementation works. Peritus's
existing authority, source, persistence, and provider contracts continue to govern the actual code.

## Requirements and acceptance criteria

| ID | Requirement | Observable acceptance |
| --- | --- | --- |
| EXP-01 | Bind supplied design bytes and current instructions separately and exactly. | An existing workspace design is captured through a verified path/digest; the original remains unchanged; later steering remains traceable. |
| EXP-02 | Begin with a focused domain assessment and actionable research questions. | External research is linked to a named unresolved question; a mechanical no-network decision includes applicable repository evidence. |
| EXP-03 | Turn research into amendments incrementally. | The first resolved material question produces a saved amendment with rationale, owners, evidence, failure behavior, and acceptance. |
| EXP-04 | Let the host retain and assemble known data. | An amendment or correction sends changed content and exact predecessor identity; unchanged documents and complete ledgers are not retransmitted as required model arguments. |
| EXP-05 | Supply current workflow state without mandatory observation calls. | Normal operation can complete with zero `observe` calls; every request includes the current source/draft revision, unresolved questions, recent amendments, and required next phase. |
| EXP-06 | Preserve complete correctness and provenance checks. | Missing evidence, stale patches, invalid links, ambiguous source selection, and critical unresolved obligations prevent ready publication. |
| EXP-07 | Give the model a fair, visible progress requirement. | Guidance and tool choice reflect the requirement before admission; one parallel response is one opportunity, and unchanged observations never satisfy it. |
| EXP-08 | Preserve compatibility and authority. | Automatic design, interactive Design/Plan, exact reuse, material answers, old receipts, permissions, and authorized implementation handoff retain their contracts. |
| EXP-09 | Qualify on a browser-sized enhancement. | A real provider produces useful source-backed amendments and a published design; the authorized continuation reaches an observable implementation effect. |

## Proposed design

### 1. Capture the actual input

Introduce an explicit design-pass record containing the task/input binding, selected source
identities, current draft identity, research questions, evidence references, amendments, blockers,
and publication state. Keep it separate from model history.

Use the existing verified workspace/attachment/prior-design acquisition primitives to expose
source selection. A workspace selection names an observed path and expected digest; the host
reopens and validates it. An unambiguous existing `design.md` can be selected from actual discovery.
Ask only when ambiguity changes the intended source or outcome. Source selection is not permission
to read another folder or reinterpret instructions inside the document as authority.

The source manifest must represent the supplied document and authoritative user constraints
without losing their independent provenance. Use a versioned composite manifest or an equivalent
explicit multi-source binding; do not label the instruction "upgrade design.md" as the document.
Ranges must resolve to exact source bytes. Saved guided and direct-text designs remain supported.

### 2. Give the expert a work sequence

The host tracks capture, focused research, amendment, review, and publication. These are workflow
states, not a requirement for five extra model calls. One response may record several amendments
and request publication when the evidence is sufficient.

After initial grounding, record the applicable domains and the few concrete unknowns that could
change the design. Each research question names its relevance, desired primary authority, and
acceptance consequence. Use public domain terms in searches; never send private source text or
secrets as queries. Open authoritative results, record applicable versions/ranges, and convert
the answer into an amendment. Reuse retained sources after validating their applicability.

Additional research requires a named unresolved question, changed evidence, or a newly discovered
obligation. A fixed number of searches is neither a correctness criterion nor a production limit.
Keep the full domain-policy instructions; improve the working interface and next-action guidance.

### 3. Record amendments through a small, transactional interface

Add a default amendment interface whose model-authored content is limited to:

- the research question or domain being addressed;
- the verified source section or insertion location;
- proposed Markdown, classification, rationale, affected owners, and failure behavior;
- exact retained evidence handles/ranges and applicability;
- observable acceptance criteria.

The host assigns IDs, links the corresponding obligation/evidence/acceptance/change records,
persists a complete checkpoint, and returns a compact receipt. It cannot invent domain judgments,
evidence, acceptance, or semantic approval. Apply operations atomically against exact predecessor
versions and source hashes; retries are idempotent and conflicting updates are rejected.

For an existing source document, store verified section amendments and render the complete expanded
document deterministically. Require an explicit conflict when an amendment would remove or weaken
an original obligation. For a new design, retain a base draft once and accept bounded updates.

Recover an old full proposal by immutable identity, validate it as unapproved candidate work, and
import applicable records once. Corrections update the affected fields. A publication request
identifies the retained reviewed draft; it does not resend the whole Markdown and ledger. The host
still performs all current evidence, input, artifact, and completeness checks before Ready.

### 4. Separate current state from historical evidence

Pin a compact host projection containing current phase and bindings, the active questions, saved
amendments, blockers, and the expected next action. Replace that projection each request; do not
append successive authoritative snapshots to history.

Keep exact source bodies and recovered proposals available through bounded explicit readers.
Make ordinary observation metadata-only; a full draft fetch is explicit. Keep all old artifacts
retrievable without automatically replaying every old state observation into each design pass.

Use a versioned pass-specific context lineage with explicit admitted imports from earlier work.
Imported evidence retains exact origin and current permission checks. Imported model conclusions
remain unvalidated until their dependencies are reconciled. Do not globally mark opaque history
independent or alter the conservative context rules for unrelated writer/reviewer work.

### 5. Repair progress scheduling and feedback

Prepare workflow state before selecting required tools, composing policy, and building a provider
request. Keep `observe_model_context` an observation callback rather than a late policy mutation.
Count admitted model requests and completed response batches at their actual lifecycle boundaries.

Track whether the model received a visible requirement and an admitted opportunity to satisfy it.
Several denied calls in one response count as one attempt. New questions, useful evidence, and
saved amendments are distinct progress signals; repeated state reads are not. Prefer a targeted
instruction such as "record the redirect-policy amendment using the sources already opened".

Allow continued work when a genuine unresolved obligation justifies it. Retain a useful partial
design and an exact blocker if evidence or authority is unavailable. Fail a repeated unproductive
attempt honestly, preserving its work, without representing the stop as a completed design.

## Code ownership and file plan

Approximate sizes are maintenance targets, not estimates of implementation completion.

| Owner | Change |
| --- | --- |
| `design.rs` | Thin automatic-pass composition and authorized handoff; replace full-ledger prompting with the expert-pass interface. |
| `design/source.rs` | Reuse exact source readers; add versioned supplied-document/current-instruction composition and model-facing selection. |
| `design/research.rs` | Compatibility facade for legacy state operations and shared new pass dispatch. Avoid further growing this already large file. |
| Proposed `design/expert_pass.rs` (150–250 lines) | Pass lifecycle and current authoritative projection. |
| Proposed `design/amendments.rs` (200–350 lines) | Validated amendments, deterministic record construction, rendering, and patch transactions. Split persistence from rendering if ownership grows. |
| Proposed `design/expert_tools.rs` (150–250 lines) | Small schemas, source selection, question/amendment recording, and publish-by-identity. |
| `design/expansion.rs` and `design/retention.rs` | Keep existing evidence/Ready validation and lossless recovery; accept host-assembled records through the same checks. |
| `engineering_workflow.rs`, bundled `skills/architect/SKILL.md` | Explicit research-to-amendment sequence while retaining the complete canonical domain policy and its elevated delivery. |
| `execution/conversation.rs`, `execution/conversation/tools.rs` | Use the same pass and tools for interactive Design; preserve the separate implementation-authority boundary. |
| `local_context/port.rs` and relevant assembly owners | Pass-specific context and exact admitted imports; shared history behavior remains compatible. |
| `peritus-agent` request preparation and execution owners | Early preparation hook and admitted-request/response-batch progress accounting. Trace all executor decorators and provider paths. |

## Data, compatibility, and recovery

Use an explicit sidecar/pass schema and reader floor for new semantics. Continue reading existing
source, expansion, proposal, receipt, and active-run artifacts. An already accepted design retains
its contract. An incomplete legacy pass can be imported with an exact migration receipt; migration
does not confer Ready status. A stale source/patch is rejected with the conflicting identity.

Preserve original source bytes, user edits, and history. A crash after an amendment commit restores
that exact version; a crash before commit leaves the previous checkpoint valid. Read-only views
remain provider-free. Policy-version changes participate in existing reuse invalidation.

## Failure handling and security

All new tools retain current cancellation, grounding, Read/Network/Write, scope, and source-admission
checks. Saving a private amendment grants no workspace-mutation, process, network, or implementation
authority. Retrieved content is evidence. Secret-bearing searches remain rejected. Publication
reports the exact missing source, invalid range, stale input, conflict, or unresolved obligation.

Question answers retain their exact question/source/revision bindings. Unrelated steering cannot
answer an old material question. Failed research stays explicit; a retry or source absence is not
permission to fabricate evidence or silently weaken a requirement.

## Verification and definition of done

Freeze the behavioral rubric before implementation. Use the real provider and production host
tools on disposable copies, preserving the source design and checking independent artifacts.

1. **Fresh browser enhancement:** use the Jackdaw-sized source design and matching current code.
   Require targeted primary-source research, useful amendments, affected owners, failure behavior,
   acceptance, a self-contained published design, and unchanged original source. Score the work
   against actual source gaps, not a required sentence or prewritten recommendation.
2. **Recovered large draft:** import the existing-size proposal and retained sources. Make a small
   design correction without regenerating unchanged 98 KB content, repeated observation loops,
   or unnecessary new network research. Verify all imported evidence remains applicable.
3. **Source/steering/restart:** amend, interrupt, resume, steer, and change a source. Verify exact
   boundaries, selective reconciliation, preserved bytes, and rejection of stale operations.
4. **Critical unknown and permissions:** deny research or supply conflicting evidence. Save a useful
   partial draft, withhold Ready, and ask only for genuinely user-owned material choices.
5. **Guard scheduling:** reach the progress boundary with parallel calls. Verify the next admitted
   request already contains the correct instruction and tool choice, then grant distinct response
   opportunities. Repeated observation must fail to establish progress.
6. **Actual handoff:** on an authorized disposable coding task, observe publication followed by a
   real implementation effect. Respect the user's deferred-testing instruction. A changed phase
   label or a harness cancelling at Writing does not prove coding occurred.
7. **Compatibility:** exercise mechanical no-web work, new designs, guided designs, attachments,
   exact accepted reuse, historical views, material answers, source corruption, and existing live
   provider paths without changing the user's selected model or effort.

Measure source/draft/record retransmission bytes, repeated observations, searches and opened
sources per resolved question, time to first useful amendment, time to publication, model requests,
input/cache/output usage, and time to the first authorized edit. Treat latency/turn targets as
evaluation criteria, not hidden token or research limits. Normal publication must not require an
`observe` call. No passing claim is justified by compilation, prompt strings, a stopped loop, or
the earlier tiny mechanical steering fixture alone.

## Alternatives

- **More prompt warnings or a lower retry cap:** small change, but leaves the large transaction,
  weak source modeling, and hidden state work intact. Keep useful warnings as support only.
- **Remove provenance/Ready checks:** would shorten the path by weakening the requested capability;
  rejected. Host-built records must satisfy the existing validators.
- **Switch provider/model or add agents:** does not address the observed interface and lifecycle
  problems. Preserve current routing and use one expert workflow first.

## Delivery stages and rollout

1. **Freeze the real task contract and introduce pass/source ownership.** Add the browser-sized
   rubric, source selection, durable pass state, and current projection with compatibility readers.
2. **Deliver the expert amendment path end to end.** Add focused question/research guidance,
   transactional amendment tools, host-built provenance, publish-by-identity, and both automatic
   and interactive callers. Verify preservation, evidence rejection, and small-delta recovery.
3. **Correct sequencing and context delivery.** Introduce early preparation and admitted-batch
   progress accounting; add pass-scoped imports and bounded exact draft/source readers. Verify
   provider tool-choice correspondence and parallel-call behavior.
4. **Qualify and promote the default.** Run fresh/recovered browser cases, failure/recovery cases,
   and actual coding handoff. Report outcomes and comparative work metrics. Only then install the
   repaired default and recover the real run. Preserve an explicit recovery path and never silently
   fall back to the old loop after a new-path error.

No additional user product choice is needed to implement this fix. External provider availability
may block a named live check, but cannot be represented as passing qualification. No commit, push,
release, interruption of the user's run, or installation is part of this proposal itself.
