# Feature: Deep research

## Summary

Add a durable, provider-neutral native Rust Research plugin that turns a question into an editable research plan, gathers and compares real sources, and delivers a cited Markdown report. It works for general topics and as an input to guided Design. Use the shared module foundation, existing built-in Rust search, source archive, provider routing, live conversation, and daemon lifecycle. Research is an installable Rust code plugin published in the official Module Market, using native framed IPC. Its Rust feature implementation is separate from the Peritus binary.

User decisions, 2026-10-17: clarify the request and show a plan first; **wait for Start research before gathering sources**. Ordinary questions and the existing lightweight web tools remain available without starting a research job. The installable native Rust crate owns its coordinator, plan/evidence ledger, role schemas, and report lifecycle in the Module Market repository. Peritus supplies generic process hosting, models, permitted tools, and action menus. The broader requirements below remain a roadmap where not exercised; see `.design/module-market.md` for the delivered boundary.

Source quality is an explicit user requirement: unvetted blogs and similar commentary are discovery leads, not evidence for conclusions. Search position, popularity, repetition and publisher prestige do not establish credibility. Admit evidence for a specific claim and preserve the reason it qualifies; report insufficient evidence rather than lowering the standard.

## User-visible behavior

1. **Clarify.** `/module peritus.deep-research TOPIC` through its plugin action menu asks only material unanswered questions: the decision or question, audience/output, scope, time period, and required/excluded sources. Reuse answers already in the conversation. Offer short suggested answers and free text. No fixed questionnaire.
2. **Plan.** Show the question, proposed subquestions, source strategy and admission criteria, expected report shape, and assumptions. The user can revise them conversationally. Save the plan before saying it is saved. Show **Start research**; preparation alone does not start web discovery or fetches.
3. **Research.** After Start, show readable progress: which question is being investigated, sources actually opened, findings, disagreements, and remaining gaps. Users can steer, pause, resume, stop, or request the current report. Do not present private model reasoning as progress.
4. **Report.** Deliver an executive answer followed by the requested analysis, source-linked findings, disagreements, limitations, and references. Distinguish observations, source claims, inference, and recommendations. Readers can inspect the retained passage behind a citation. A report may finish with unanswered questions if evidence is unavailable; say so clearly.
5. **Reuse.** Save/report browsing survives client or daemon restart. Export Markdown on request. **Use in design** attaches a specific report version to a guided design; it does not start implementation. Further research creates a new report version and preserves earlier citations.

Research must work in a repository, a plain folder, and without a coding project. Standalone mode uses an app-owned empty read-only context if the current product request still requires a workspace identity; it must not scan the user's home directory or initialize a Git repository. It uses the currently selected provider/model and existing reviewer selection, with no search API key or container/interpreter prerequisite for normal search and reading.

Proposed terminal commands: `/module peritus.deep-research`, `/module peritus.deep-research start`, `/module peritus.deep-research plan`, `/module peritus.deep-research status`, `/module peritus.deep-research show`, `/module peritus.deep-research pause`, `/module peritus.deep-research resume`, `/module peritus.deep-research stop`, and `/module peritus.deep-research use-in-design`. Topic text after `/module peritus.deep-research` starts preparation. Export uses an explicit destination and the normal host file-write permission; browsing a private report requires no workspace write permission. Provide equivalent browser controls, an expandable plan/source list and a readable report. Command parsing and typed actions must agree; research controls cannot be mere text that the model can accidentally ignore.

## Requirements

| ID | Requirement |
| --- | --- |
| R1 | Research is explicitly invoked, begins with clarification/plan, and waits for a user Start bound to the current plan revision. |
| R2 | Support general topics and a linked Design question without granting coding/build authority. |
| R3 | Investigate each planned question with real source reads; search snippets and links alone are discovery leads. |
| R4 | Retain source provenance and exact passages; citations resolve to the source snapshot actually read. |
| R5 | Seek relevant contrary evidence and track disagreement, scope/date differences, and source independence. |
| R6 | Save incremental progress and resume after disconnect/restart without restarting the investigation. |
| R7 | Accept steering and provide predictable pause/stop/resume/current-report controls. |
| R8 | Finish on evidence-based coverage of the plan, with explicit unknowns; avoid endless reading/repeated queries and arbitrary cumulative token/time quotas. |
| R9 | Bound concurrent work, transient memory, context pages and individual network/parser operations while allowing retained evidence to grow on disk. |
| R10 | Produce a useful saved Markdown report, source inspection, explicit export, and versioned Design handoff. |
| R11 | Preserve existing Chat, Plan, Review, Design, Build, provider routing, permissions and binary/session recovery behavior. |
| R12 | Read ordinary HTML/text and text-bearing PDF primary sources; report unsupported/scanned/encrypted/failed reads honestly. |
| R13 | Admit evidence per claim using recorded provenance, authority for that claim, methods where relevant, date/version, independence and limitations. Unvetted blogs, SEO articles, social posts, aggregators and AI summaries remain discovery-only. |
| R14 | Only admitted evidence can support conclusions or count as independent corroboration. Preserve unknown/insufficient-evidence outcomes without substituting lower-quality sources, prestige, search rank or repetition. |

## Acceptance criteria

- A fresh user can prepare a question and edit the saved plan in TUI and browser. No search request or source download occurs before Start. A stale Start after plan edits does not run the old plan.
- Start performs discovery and opens actual sources. A source-backed finding includes a resolvable snapshot and passage; fabricated IDs/ranges are rejected as correctable tool input, without terminating the run.
- A controlled source disagreement appears in the report with both passages and their scope/date differences. Syndicated copies do not count as independent confirmation.
- A highly ranked unvetted blog, SEO article or AI summary does not become supporting or contrary evidence merely because the agent opened it. It may lead to an original source; only separately assessed, admitted passages can support a finding. Missing admission or an ineligible claim/citation pairing returns a correctable response and leaves other research usable.
- A company announcement supports an attributed statement about what the company announced. The same passage cannot establish independently measured superiority or performance. Ten copies of a claim trace to one evidence chain, and the report does not describe them as ten confirmations.
- A prestigious publisher's unsupported assertion does not qualify automatically. When only discovery-only material is available, the report states unknown/insufficient evidence and does not mark the question supported or invent consensus. Source browsing shows why each admitted passage qualified and its limitations.
- Client reconnect, explicit pause/resume and daemon restart retain plan version, answered/open questions, consumed sources, findings and draft report. Restart reconciles unfinished read work and presents a resumable job, without silently authorizing new provider calls. Client loss alone does not pause it.
- Steering changes future work and report framing, invalidates stale synthesis, and retains historical sources. A major scope revision returns to plan review; it does not keep researching an obsolete brief.
- Blocked engines/pages, unsupported PDF content and unavailable providers produce visible limitations and retry/recovery options. A completed report can contain reasoned unknowns. Local persistence failure preserves the last committed state and never reports an unsaved result as saved.
- A looped query/URL sequence is recognized as no new evidence: choose an alternate source, synthesize the gap, or ask a material question instead of repeating indefinitely. Useful ongoing work continues without an arbitrary total token ceiling.
- A growing archive and large report remain pageable, do not require loading the full history into a request, and do not monopolize daemon heartbeats or a second client. Stop acknowledges cancellation and preserves a labeled partial report.
- Opening a report in a fresh client resolves the original citations. An explicit refresh produces a new snapshot and report revision, without changing the meaning of earlier references.
- A specific report version can be attached to Design. A later `/design build` receives that selected research with the saved brief; research itself never starts Build, commits, or changes project files.
- A non-coding topic can be researched without a repository. Normal Rust search/reading works with no Docker/Python on PATH. Existing provider selections and coding conversations continue to work.

## Current architecture

Inspected on clean `develop` at `fab0201dd983ae9ca5416f2036c9dfe5eefe3e4f`.

| Existing owner | Relevant implementation and consequence |
| --- | --- |
| Product request/mode | `crates/app/peritus-app-protocol/src/product/interaction.rs`: append-only tags Chat=1, Plan=2, Review=3, Build=4, Design=5. Mode/schema changes require coordinated projections. |
| Daemon task ownership | `crates/app/peritus-daemon/src/product_run/execution/launch.rs`: `ProductRunService::spawn` owns the task, cancellation, provider composition and live `ConversationView`; synchronous coordination is kept off connection/heartbeat workers. Reuse this ownership. |
| Live inputs and authority | `crates/app/peritus-daemon/src/product_run/interaction/live.rs` and `live/request.rs`: revisioned input, durable provider admission, current permissions, and public activity. New actions must use the same authenticated binding. |
| Conversation runner | `crates/app/peritus-product-runner/src/execution/conversation.rs`: read-only tool-capable invocation, separate working-memory lineage, continuation after bounded segments, and ordinary final prose settles a conversation turn. A research job needs explicit durable stage handling beyond that final-prose rule. |
| Tool boundary | `execution/conversation/tools.rs`: `ConversationTools` wraps read-only `WorkspaceDeveloperTools`, adds the private `design_draft` tool, and only Chat may request pipeline handoff. Research must never receive `run_pipeline`. |
| Guided Design | `guided_design.rs` and `execution/conversation/design.rs`: run/workspace-bound versioned artifact, expected-version update, stale-input rejection, atomic save and explicit downstream implementation context. Reuse identity/revision patterns, not the single full-document replacement shape. |
| Web evidence | `crates/tools/peritus-tools-web/src/{source,http,search,extract}.rs`: built-in Rust metasearch, HTTP(S), immutable source identity, integrity, exact UTF-8 paging/find, timestamps, URLs and cache honesty. Native search currently supports first result sets and explicit engine subsets, not native date/language/pagination filters. PDF is currently rejected. |
| Web dispatch | `crates/app/peritus-product-runner/src/developer_tools/web.rs`: provider-neutral schemas; live Read/Network checks; optional isolated rendering additionally needs Process. Preserve this boundary. |
| Working memory | `local_context/{driver,pages,storage,tools}.rs`: `run_live_invocation` records exact observations and uses paged checkpoints/artifacts rather than keeping every source in provider context. Reuse the invocation path and persistence patterns. |
| Reviewer | `reviewer_turn.rs`: a fresh read-only role with selected reviewer provider and its own context. Reuse role routing/context isolation, not the software-candidate review schema or candidate qualification. |
| Clients | TUI `src/{modes,help}.rs`, `src/model/chat/{commands,catalog}.rs` and `catalog/entries.rs`; WebUI `src/lib/{types,modes}.ts`, command catalog, conversation controls; gateway `crates/app/peritus-web/src/daemon.rs` maps mode strings. Research must appear consistently across these surfaces. |

Reading these owners established the boundaries; this was not a whole-codebase audit. There is currently no durable research plan/finding/report owner, source-to-claim ledger, research lifecycle, PDF text path, or explicit report-to-Design reference.

## Proposed design

### One durable research session

The delivered core uses explicit `/module peritus.deep-research` host commands and the module runner inside `peritus-product-runner`, launched/owned through the existing `ProductRunService`. A separate mode tag is not needed for the installed workflow. Proposed new `research.rs` / `research/` modules own domain records, plan transitions, scheduling, citation references and report publication. A small conversation adapter supplies preparation/questions and research-specific artifact tools. These are proposed files, not existing implementation.

Treat research as a daemon task within the current lifecycle, not a second daemon or a detached process. For Design integration, create a linked research run with its own identity and return destination; the parent Design draft remains intact. Only explicit **Use in design** attaches the resulting version. Standalone and linked entrypoints invoke the same coordinator.

Persistent session stages: `clarifying → plan_ready → researching → synthesizing → complete`, with explicit `paused`, `stopped`, and `needs_attention` observations. Completion describes a finished report, not proof that every question is answerable. Each question has `open`, `supported`, `contested`, or `unknown` disposition with evidence/limitations. Stop is retained and resumable; it does not erase the investigation.

Start is a typed user action bound to session/run/workspace identity, input revision and saved plan version. Model text or a retrieved page cannot supply it. Preparation can use supplied material already in the conversation; it cannot invoke research web tools before Start. Any private-state writes remain confined to app-owned artifacts.

### Required agent workflow

The user selected this sequence: **gatherer → skeptic → synthesis → final skeptic fact-check → report**. Register these roles/stages through the module foundation rather than implementing an unrelated runner.

- **Gatherer:** investigate the approved plan and collect candidate information with original sources, exact passages and provenance. Collection is not admission; its candidates cannot directly reach synthesis as accepted facts.
- **Skeptic:** independently inspect those passages and the source-admission policy. Reject unsupported/ineligible claims, track rejection reasons, assess contrary evidence and preserve unknowns. Its structured accepted/qualified evidence set is the synthesis input.
- **Synthesis:** write the report from the skeptic's accepted evidence, permitted qualifications and explicit gaps. It cannot silently reinstate rejected material or turn uncertain claims into facts.
- **Final skeptic:** use a fresh review context to fact-check the actual report against admitted original passages, scope/date/version, inference, independence and citations. Return specific corrections or an explicit publishable result. Corrections go back to synthesis, and substantive changes return to the skeptic before presentation.
- **Presentation:** publish the fact-checked report and visible limitations. If a material issue remains unresolved or checking is unavailable, expose a labeled partial draft/status rather than presenting unchecked claims as a completed fact-checked report. Lack of qualifying evidence is an acceptable explicit result, never a reason to promote rejected material.

Each role has separate working memory and host-enforced artifact/input access. Gatherer candidates, skeptic decisions, synthesis drafts and final checks are versioned. Final approval binds the exact report and evidence versions; later changes invalidate that approval. Existing provider selections serve these roles without exposing credentials to module packages. The skeptic uses the configured reviewer model by default; gatherer and synthesis use the writer model, using the shared module contract's writer/reviewer routing. Dedicated per-module model overrides can extend that contract.

### Source admission and what counts as evidence

Search ranking is a discovery mechanism. Agreement between search engines does not establish accuracy, and opening a page does not qualify it as evidence. Apply the plan's source criteria before using a passage to support a conclusion, weigh a contradiction or describe independent corroboration.

| Candidate | Default treatment |
| --- | --- |
| Original study with assessable methods/data and relevant systematic review with assessable selection/synthesis methods | Eligible for assessment of the specific research claim; record methodology, scope and limitations. Publication or peer-review labels alone do not establish adequacy. |
| Original official dataset/report, standard, versioned documentation or source code | Eligible for claims within its actual authority and scope. Separate a specified/documented behavior from independently observed execution or outcomes. |
| First-party announcement or vendor statement | Eligible for a narrowly attributed account of what its author announced or claimed. It is not independent confirmation that the claim is true or that a product is superior. An official release announcement may use a blog format; its admissible scope remains narrow. |
| Unvetted personal blog, SEO article, social post, aggregator or AI-generated summary | Discovery-only. Follow useful references to their originals; the page's assertions cannot support conclusions or count as contrary evidence/corroboration. |
| Material outside explicit source restrictions, unverifiable attribution, or a passage that does not support the proposed claim | Excluded from evidence for that claim, with a recorded reason. Preserve a useful discovery link only when permitted by the plan. |

For each proposed claim-to-passage admission, record:

- The claim/question, exact source passage and source identity/origin.
- The source category, observed publisher/author, and why this source has authority for this particular claim.
- Relevant publication/update date or version, observed retrieval time, and whether applicability to the requested period/version is established. Missing metadata remains unknown.
- Assessable methods/data where the claim requires empirical support, including whether the statement is self-reported, measured, inferred or a synthesis.
- Independence and the original evidence chain: copied, syndicated, quoted or mutually dependent pages cannot multiply corroboration.
- Limitations, conflicts of interest or relevant corrections/retractions/supersession when observed, and the scope in which the passage may be used.
- Admission state and a concise public rationale: unassessed/discovery-only, admitted for the named claim, or excluded for that claim.

Authority is claim-specific, not a global trust score for a domain or author. Domain/source restrictions can narrow eligibility; an allowed domain does not automatically admit everything it publishes. A source eligible for one claim may be ineligible for another. Scientific conclusions cannot gain equal-weight opposition from an unvetted opinion page merely to manufacture balance. Qualifying contrary evidence receives the same admission scrutiny as supporting evidence.

The host requires an explicit current admission record before a citation can serve as support/counterevidence or a question can be marked supported/contested. It checks that the record matches the actual passage, claim, plan version and allowed category; it must not accept an arbitrary source hash plus a generic "credible" label. These checks enforce provenance and policy structure, not truth: the research model and focused source check still have to assess relevance, methods and inference from the underlying text. Keep their rationale inspectable rather than representing model confidence as a measured credibility score.

If qualifying evidence cannot be obtained, retain the lead and the retrieval/admission limitation and report unknown or insufficient evidence. Do not silently promote a blog, search snippet, inaccessible citation or repeated assertion to fill a report section or satisfy a source-count target. Any user-requested change to source criteria is a visible plan revision; it never turns dependent evidence into independent corroboration.

### Plan and evidence records

Retain small versioned records, not one ever-growing JSON/Markdown object:

- Session manifest: identity, parent/return destination, stage, governing input/plan versions, selected model references, report versions, checkpoint and next work.
- Plan: main question, subquestions, scope/assumptions, source strategy and admission criteria, required/excluded sources, time framing, report shape and question relationships.
- Work item: question/query or candidate source, lifecycle, attempt/result provenance and whether it added information. Unfinished reads are safe to reconcile/retry; committed observations are reused.
- Source: original/final URL, observed title/author/publisher/date/version when actually available, retrieval time/method, existing immutable source ID/digests, category, aliases/duplicates, extraction limitations, source origin and evidence-chain identity. Missing metadata stays unknown; source-level metadata alone does not admit a claim.
- Passage: source ID, exact UTF-8 range and optional PDF page, content digest, and the real read observation that introduced it.
- Admission: claim/question and passage IDs, governing plan version, admissible scope, state, authority/methods/date/independence assessment, limitations and public rationale. Changes retain history and invalidate affected finding/report eligibility.
- Finding: claim, question IDs, supporting/contrary passage IDs and their admission records, provenance, qualification and explicit interpretation/uncertainty. Model estimates are not measured probabilities.
- Report: versioned sections and citations, question disposition summary, limitations, references and completeness label.

A research tool wrapper records successful `web_open`/`web_read` observations and their exact ranges as unassessed material. Model-submitted findings may reference only passages actually available to this research session and admitted for the proposed claim; arbitrary URLs/source hashes do not become evidence by assertion. Explicit reuse from another report first resolves its saved source/passage through the normal read path and reassesses admission against the current claim/plan. Search results remain leads.

Tools should offer small typed operations such as read/update plan, record finding, inspect question/source index, and draft/finalize report. The host validates identity, revision, source/range integrity and state transitions. It cannot mechanically prove that prose follows from a citation; semantic checking and honest qualifications remain necessary.

### Research loop and stopping

Use successive focused work items: search a subquestion, open candidate sources, read relevant sections, apply claim-specific admission, retain eligible findings, seek qualifying contrary evidence and update the question state. Apply the source-admission rules above rather than merely preferring primary sources. Trace discovery leads toward their original evidence when available. Group copied/syndicated material by evidence chain and avoid treating mirror count as corroboration.

The next invocation receives current plan, open questions, a compact evidence index and selected findings. Retrieve exact archived pages when needed. Raw history and entire documents do not have to fit in one provider request. New user instructions supersede stale plans and report drafts through the existing input revision checks.

Host progress observations distinguish a new passage, answered question, resolved disagreement, useful source rejection and newly identified gap from repeated activity. Repeated identical queries/URLs with no changed evidence become a scheduling reason to change strategy, synthesize a limitation, or ask the user. Do not implement a fixed request-count abort or a "read N sources" completion rule.

Finish when each planned question has a defensible disposition, important claims point to opened and admitted evidence, qualifying contradictions are discussed, and there is no identified useful next step within scope. Discovery-only material cannot satisfy coverage. Blocked/unavailable or inadmissible evidence can produce unknowns and a partial report; it must not yield an endless retry, lowered source standards or a claim of factual absence. Continued progress may run for hours until naturally finished or the user stops it. User-selected horizons can be respected if later offered; there is no default cumulative token/time budget.

### Synthesis and source checking

Draft the report from admitted findings and exact passages. The final skeptic performs a focused source check using the configured reviewer model and a fresh context: inspect claim-specific admission/rationale, actual cited passages, methods/authority appropriate to each claim, original evidence chains, omitted qualifying contradictory evidence, overstated conclusions and missing question coverage. Check for self-reported claims presented as established facts, citation laundering through secondary pages, and discovery-only material promoted into evidence. This checks the research output; it does not run the coding design/check/reviewer/fixer pipeline or repository gates.

Bound correction attempts by useful change, not an arbitrary demand for a flawless approval. If an issue cannot be resolved, retain the contested claim as qualified/unknown and publish the limitation. Source-check/provider failure cannot discard the report; expose its status. A user can always request the current partial report.

Stable human citations use original source URLs plus labeled references. The app additionally binds each reference to the exact stored passage, date and source/report version. Preserve original PDF page numbers. Structural citation errors are correctable tool responses; they cannot become terminal task-file/candidate blockers.

### PDF and optional rendering

Extend the C4 reader with native Rust extraction of text-bearing PDFs and page provenance. Evaluate the page-oriented APIs in [pdf-extract's primary documentation](https://docs.rs/pdf-extract/latest/pdf_extract/) against real single/multiple-column, Unicode, large, malformed, encrypted and scanned documents before selecting/pinning a dependency. The documentation establishes API availability, not extraction quality.

Parse downloaded bytes through a resource-owned, cancellable boundary; retain raw digest and extraction limitations. Never treat an empty/scanned extraction as proof the document is empty. No Python/Poppler setup should be required for text extraction. OCR, diagrams and difficult table interpretation remain explicit limitations unless separately implemented and exercised. Prefer publisher HTML when it is the authoritative equivalent and explain the source used.

Existing `render=true` remains an optional fallback for script-dependent pages, with its current Process permission and automation-runtime requirement. Lack of that optional runtime yields a readable source limitation and alternatives; normal research must not require it. Research does not attach to personal browser profiles or automate logins/purchases.

### Resources and recovery

Start with one active research model invocation per session, one native search fan-out per process and one rendered browser read at a time using existing ownership. Apply a daemon-wide resource-aware admission queue so multiple research sessions and coding work cannot each consume an unbounded allowance. Scheduling bounds concurrency and transient allocations, not retained progress or total token use. Use existing availability observations for future bounded parallel fetches; do not introduce a mandatory worker fleet.

Checkpoint after each committed plan/finding/source disposition/report section. Use append-only/paged artifact records and an atomically replaced small manifest, following existing C0/checkpoint patterns without creating another writer for the current working-memory store. A single research owner serializes mutations. Plan updates and report publication use expected versions; avoid appending large source text to ordinary run records. Exact artifact reads remain paged.

Client disconnect leaves daemon-owned work running. Explicit pause prevents further work admission and reconciles in-flight requests before confirming paused. Stop cancels owned work and commits a partial-state checkpoint. Daemon restart reloads the checkpoint and exposes Resume; old provider requests are not assumed to have completed and no duplicate task is launched. On permission loss, stop admitting disallowed work and preserve evidence; when allowed, resume from it. Existing request cancellation, provider recovery/failover and persistence diagnostics are reused rather than duplicated.

## Data and compatibility

Append the interaction mode tag; preserve all existing values. Update the A3 codec/schema/JSON projections, domain workbench mapping, daemon routing, CLI/TUI/WebUI modes and help together. Inspect all exhaustive matches and generated projections at implementation time. Older peers reject an unknown mode rather than treating it as Build.

Add versioned research artifacts independent of existing coding candidates and guided drafts. Existing runs without research remain unchanged. Linked research stores parent identity and selected report version explicitly. Design receives a compact cited summary plus retrievable report/source references, and the selected version is carried into its later implementation context; no opportunistic scan of ambient Markdown files.

Version report snapshots and preserve prior references. Report export is an explicit file action with overwrite handling; saving private research state is not arbitrary workspace write authority. The UI's report/source view must page data independently of bounded activity entries and show omission/cursor information honestly.

## Failure handling

- Search block/CAPTCHA/rate-limit: retain the observed failure, try genuinely different permitted sources/queries, back off rather than hammering, and describe missing coverage. Do not ask users to install search infrastructure.
- Page/PDF/render failure: record actual retrieval/extraction status and alternatives. Do not cite unseen full text or pretend snippets answer the source question.
- Stale plan/action/finding update: return current version and a recoverable correction; do not fail the entire research run or start an obsolete plan.
- Provider/context failure: preserve checkpoint, compact/select pages through the existing local-context path, use configured recovery, and expose Resume when user intervention is required. Never restart from an empty brief.
- Storage failure: retain the last committed manifest and visible unsaved output; stop claiming durable progress until publication works. Distinguish missing, corrupt and unreadable state.
- Insufficient/conflicting evidence: publish a qualified finding or unknown, not a fake consensus or an application failure.
- Ineligible source or claim-to-passage use: retain its discovery/admission disposition, return a correctable response, and seek qualifying evidence or publish the gap. Never relax source standards to finish, and never turn source rejection into a terminal run failure.

## Security considerations

Research tools have Read/Network and narrowly scoped private artifact authority. File export requires explicit destination/authority. No `run_pipeline`, command execution, project edits, Git operations, uploads, message sending or publishing are granted by Research. Existing rendered reads separately require Process. Recheck effective permission and governing revisions on tool/action admission, including cached reads.

Pages, PDFs, search results and report imports are untrusted evidence. They cannot change the plan's authority, supply Start, instruct local reads, disclose secrets or authorize new actions. Query generation must not send private attachments, repository text or credentials to search engines merely because they appear in model context. Use public question formulations; ask before externally searching sensitive supplied material when the requested scope does not authorize it. Respect explicit domain/source exclusions, unsafe URL admission, current profile isolation and retention boundaries. Never label fetched content as a new observation when it came from cache.

## Verification

Use actual programs/interfaces with independent expectations and disposable state. No unit-test generation, mocks of the search/research implementation, formal verification or repository verification suites.

| Campaign | Independent observable evidence |
| --- | --- |
| Preparation and revision | TUI/browser submitted topic, clarification, edit, stale Start and valid Start; tool traces plus independent outbound/request capture show zero search/source requests before valid Start; provider clarification traffic is expected. |
| Source reasoning | Controlled ordinary HTTP source documents contain known distinct dates, claims, contradictions, mirror links and injection text. The real coordinator must read and cite the right passages, preserve disagreement and ignore injected instructions. The documents are data, not a fake search/research implementation. |
| Evidence admission | Ordinary source documents include an unvetted blog with a useful original reference, an unsupported prestigious-publisher assertion, a vendor announcement/performance claim and ten syndicated copies of one original claim. Observe discovery-only handling, separate admission of the original, narrow attribution, one evidence chain and inspectable rationale. Repeat with only ineligible sources: report insufficient evidence, without marking the question supported or weakening policy. Exercise missing/stale admission and an irrelevant passage as correctable input, followed by a valid finding. These fixtures test behavior; their labels alone do not prove live-source credibility. |
| Live public research | Real search engines and actual primary sources for at least a technical design decision and a non-coding topic; manually inspect report citations against retained/full source content, not just tool success. |
| Citation defects | Actual tool/public-consumer requests with fabricated source IDs, unread ranges, stale versions and changed content; rejection is correctable and later valid work succeeds. |
| Document diversity | Real PDF/HTML/text, multiple pages/columns, Unicode, long lines, delayed JavaScript, unavailable/encrypted/scanned content, and exact page/range readback. |
| Lifecycle | Pause/resume/stop, client death/reconnect, daemon shutdown/restart, provider failure, network outage and recovery, input steering and current-report export; inspect committed progress and no unintended owned processes. |
| Long duration and resources | Run a substantive multi-hour investigation while other real coding/client work is active. Record task progress, archive growth, resident memory, CPU, heartbeat/reconnect behavior and repetition. Distinguish an observed bounded trend from proof of no leaks. |
| Integration | Standalone research, parent Design draft, report version selection, correction followed by reattachment, then explicit Build handoff; confirm exact selected report reaches downstream input without auto-building. |
| Existing flows | Replay immediately affected Chat/Design/Review/mode/reconnect/provider paths. Preserve concrete escaped defects and final replay commands under `docs/testing/reproducers`. |

Predefine the expected facts/outputs and finite exploration dimensions before implementation testing. Report PASS/FAIL/BLOCKED and platform/engine limitations; neither a report's length nor a source count is evidence of correctness. Build with bounded jobs based on available memory; preserve live user jobs.

## Delivery increments

1. **Durable preparation and controls:** Research mode, generic/Design entrypoints, private plan state, typed version-bound Start and real client controls. Evidence: no pre-Start network activity, durable edit/reopen and no build authority.
2. **Evidence-backed investigation:** coordinator work queue, source/passages/admissions/findings, enforced claim-specific source eligibility, evidence-chain grouping, progress/repetition handling, exact source browsing and native PDF path. Evidence: actual source diversity, admission/disagreement/injection cases, live queries, cancellation/recovery.
3. **Report and Design reuse:** saved Markdown, citation/admission inspection, source checking, partial/insufficient-evidence reports, explicit export and versioned handoff. Evidence: readback/citation/admission audit and actual downstream Design/Build input.
4. **Resilience and sustained use:** resource scheduling, failures/restart, multiple clients/jobs and multi-hour research. Evidence: retained lifecycle replays, resource observations and useful forward progress.

Each increment must be reviewable and usable within its stated boundary. Do not call the complete module finished before report, lifecycle and Design integration paths have been exercised.

## Rollout and rollback

Ship matching protocol/daemon/clients. Do not replace a running user daemon as part of development tests. Initial research state is additive and app-owned; rollback leaves Markdown/source artifacts inspectable and does not alter coding candidates or provider credentials. Refuse unsupported new record versions honestly. Version any later migrations and preserve prior report/source bindings.

## Open questions

No user-facing choice blocks this design: clarification and a visible plan precede an explicit Start. Parser selection, exact typed action/projection shape and artifact API reuse remain implementation decisions to settle from current code and actual behavior. A multi-hour real campaign must be scheduled with isolated data before claiming sustained operation works.

## Out of scope

Automatic implementation/publishing, paid search services, authenticated web-account control, a separate remote research service, mandatory containers/Python/worker infrastructure, OCR/video/audio understanding, arbitrary local crawling, and claims of exhaustive or infallible research.
