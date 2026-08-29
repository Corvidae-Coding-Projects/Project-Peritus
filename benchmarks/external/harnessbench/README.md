# HarnessBench

Peritus pins Qihoo360 HarnessBench at commit
`1025086a446653702b80cfb48babbeec35db6b2c`, which contains 106 tasks. The upstream checkout is a
local input and is not committed to this repository.

Build the native adapter with two Cargo jobs:

```bash
CARGO_BUILD_JOBS=2 cargo build -p peritus-external-benchmarks --bin peritus-benchmark-agent
```

Clone or reset HarnessBench to the pinned commit, then create a local app config from
`app.example.json`. Point its result and workspace paths at a directory outside Git. Create a
dedicated environment outside Git so HarnessBench and task oracles use the same pinned tools:

```bash
python3 -m venv /absolute/path/to/benchmark-state/.venv
/absolute/path/to/benchmark-state/.venv/bin/python -m pip install \
  --requirement /absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/oracle-requirements.txt
```

The separate requirements file includes HarnessBench's PyYAML dependency and pytest, which task 016
invokes from both the agent workspace and its unchanged oracle. It also pins the real
`python-slugify` 8.x runtime and transitive `text-unidecode` package used by task 045, so dependency
compatibility is executed rather than simulated. From the HarnessBench checkout, list tasks with:

```bash
PYTHONPATH=src /absolute/path/to/benchmark-state/.venv/bin/python -m harnessbench.cli tasks
```

Run the first live task with the checked-in generic adapter configuration:

```bash
PATH=/absolute/path/to/benchmark-state/.venv/bin:/absolute/path/to/Project-Peritus/target/debug:$PATH \
HARNESSBENCH_APP_CONFIG=/absolute/path/to/local-app.json \
HARNESSBENCH_HARNESS_CONFIG=/absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/harness.json \
HARNESSBENCH_PUBLIC_URL_TEMPLATE='{local_url}' \
PYTHONPATH=src /absolute/path/to/benchmark-state/.venv/bin/python -m harnessbench.cli run-task \
  --task 001-file \
  --harness peritus-codex-claude \
  --mode live
```

HarnessBench owns task setup, timeouts, workspaces, oracles, process rubrics, and scoring. The
Peritus adapter initializes a local Git baseline only when the supplied workspace has no Git
history, runs the real product composition, and projects its durable normalized trace into the
suite's `usage-proxy` directory. It does not edit tasks, fixtures, hooks, rubrics, or oracles.
`HARNESSBENCH_PUBLIC_URL_TEMPLATE='{local_url}'` lets tasks 003 and 006 use their local fixture
servers without installing a public tunnel. It does not replace or bypass the task server.

Do not use `HARNESSBENCH_SKIP_PROCESS_GRADE` for a scored qualification run. If no compatible
rubric credential is configured, start the local rubric boundary in another terminal:

```bash
python3 /absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/rubric_server.py \
  --agent /absolute/path/to/Project-Peritus/target/debug/peritus-benchmark-agent \
  --port 8765
```

Then add these variables to the benchmark command:

```bash
RUBRIC_API_KEY=peritus-local-rubric \
RUBRIC_BASE_URL=http://127.0.0.1:8765/v1 \
RUBRIC_MODEL=gpt-5.6-sol
```

The Python process only provides the HTTP shape required by HarnessBench. Each request is passed
unchanged to the native Rust agent, which uses the authenticated official `codex` executable. The
bridge does not read or copy account credentials. Text and bounded inline PNG, JPEG, WebP, and GIF
inputs are preserved. Rust validates image data before the official executable receives private
temporary files through its documented `--image` option.

## Current qualification progress

The live baseline has exercised tasks 001 through 097 against the pinned, unchanged suite. Tasks
022, 023, and 026 complete with oracle outcome 1.0. Tasks 021, 024, 025, 027, and 028 retain lower
unchanged outcomes because of documented hidden taxonomies, invalid calendar ground truth,
unmatchable normalization, or brittle unpublished phrase checks. Task 029 similarly retains a
lower score because it requires unpublished contiguous issue labels despite correct calculations;
task 030 requires hidden change-log rows for sections that correctly remained unchanged. Task 031
requires reuse of a reference identifier that its own appendix retires, plus unpublished audit-row
formats. Task 032 treats an explicit “cannot commit” legal boundary as a promise, and task 033 calls
a directly sourced negative answer insufficient evidence. Task 034 reaches the suite's excellent
level while retaining two documented hidden-token misses. Task 035 resolves every scoped fact and
supporting artifact correctly but its priority-reason check recognizes only a narrow verb allowlist.
Task 036 exposed a real CSV acceptance gap, now closed by a native structural gate and verified by
an unchanged fresh rerun. Its remaining lower score double-counts one explicit key rename and
requires an unpublished conflicting-key convention. Task 037 exposed a premature-question and
fixer-convergence problem around an inconsistent canonical reason registry. Peritus now produces
the complete ruling artifacts, preserves superseding authority in primary clause fields, and exits
before the unchanged 900-second deadline; its retained 0.74 outcome reflects hidden exact-quote and
line-item conventions documented in the journal. External invocations also retain their final
product diff, gate report, review ledger, and finding state in `last-product-observation.json`.
Task 038 recovered automatically from one five-minute provider timeout, produced and independently
accepted all four requested research artifacts, and scored 0.8469 (`good`). Its remaining misses
come from an input-path normalization bug, an unpublished `source_rows` requirement, and one hidden
hyphen-sensitive token. Task 039 inspected a repository without changing it, produced all five
architecture and onboarding artifacts, repaired invalid CSV quoting through the normal review loop,
and scored 0.9673 (`excellent`). Its small remaining outcome gap comes from hidden substring checks
that reject the accurate terms `OrderRepository.save` and `retries`; its 0.83 process score records
redundant reads and a writer completion statement made before independent review found the CSV bug.
Task 040 initially produced a perfect external result while native acceptance correctly refused to
trust writer-run tests that were absent from deterministic gate evidence. Gate discovery now
recognizes conventional Python `tests/` projects without requiring an invented manifest path. An
unchanged rerun independently compiled the package, passed all 24 tests, completed review with no
findings, and retained outcome 1.0 with process and combined score 0.9367. Task 041 found the same
verification gap for a manifestless CommonJS module. Adjacent `*.test.js` and `*.spec.js` files now
form a deterministic Node test contract without `package.json`; the unchanged rerun passed its
native Node gate and review in one cycle. Its 0.9962 (`excellent`) outcome misses only a hidden
`schemaVersion` spelling while correctly persisting the required version as `version: 2`. Task 042
extended manifestless Python discovery from changed tests to production `.py` sources and proved
native compile, pytest, and review acceptance. Its official 0.4 outcome is retained: the unchanged
oracle crashes while dynamically loading ordinary dataclasses without registering their module.
A separately labeled diagnostic also records an internally stale audit-count expectation and an
unpublished zero-quantity rule; these are not used as the official score.
Task 043 produced a complete, executable SQLite migration and scored 0.995 (`excellent`), but its
first native report exposed that database execution lived only in the writer trace rather than the
deterministic acceptance gate. Conventional `schema.sql` plus `migration.sql` workspaces now bind
to a Rust-owned SQLite gate. An unchanged rerun executed the schema, migration twice, postcheck,
foreign-key checks, and rollback in a disposable in-memory database before review accepted the
candidate; it retained the 0.995 outcome and scored 0.9433 for process quality.
Task 044 produced a safe executable GitHub Actions workflow, preserved every protected input, and
recovered automatically from a five-minute provider stall. It exposed that root-level Python tests
and changed YAML configuration were falling through to generic artifact checks. Peritus now binds
root `test_*.py`/`*_test.py` conventions to their project, performs side-effect-free Python syntax
and pytest gates, and structurally parses changed YAML. The unchanged rerun records all four native
checks and scored 0.98 (`excellent`); its remaining gap consists of two unpublished prose-token
spellings documented in the journal.
Task 045 initially exposed a false compatibility proof: the fixture dependency was absent, a fixer
injected a substitute module into the test process, and review treated real dependency execution as
optional. Peritus now verifies conventional `requirements.txt` files with an offline, read-only pip
resolution pass and makes the changed real dependency a blocking acceptance prerequisite. The
benchmark environment pins `python-slugify` 8.0.4 and `text-unidecode` 1.3. An unchanged rerun kept
the original tests intact, independently verified the installed dependency, passed every direct
behavior check, and scored outcome 0.98 (`excellent`), process 0.9467, security 1.0, and combined
0.9277 in 351.72 seconds.
Task 046 produced a correct exact-SKU index and passed every generated correctness, edge-semantics,
implementation-shape, fixture-integrity, and performance check with outcome 1.0. It exposed two
general evidence gaps: standalone changed Python source had only generic artifact acceptance, and
performance work did not consistently compare the baseline with the candidate. Peritus now gives a
standalone non-test Python module its own syntax target unless an enclosing Python project owns it,
and its engineering workflow requires a same-workload baseline/candidate measurement. The final
unchanged run recorded native Python evidence, measured about a 9,758-times lookup improvement, and
scored process and combined 0.93 with security 1.0.
Task 047 produced all nine concrete security and regression findings with exact severity, complete
recommendations and evidence, and intact fixtures. Its first run exposed that changed JSON output
had no deterministic structural acceptance. Peritus now parses every changed JSON deliverable with
a bounded native gate before review. The unchanged rerun records that native evidence and one-cycle
review, with outcome 0.7199, process 0.8633, security 1.0, and combined 0.6215. The remaining outcome
loss is an unpublished raw-token test matcher that rejects three specific regression tests written
with equivalent ordinary language; it is documented without adding benchmark-specific wording.
Task 048 generated five mutually consistent release artifacts, deduplicated shipped issues, kept
reverted, deferred, documentation-only, and security IDs out of the product issue count, supplied
all four breaking-change migrations, and preserved the embargo. Every named oracle check passed;
native acceptance parsed all three JSON outputs before one-cycle review. The retained outcome is
0.9478 (`excellent`), with process 0.9, security 1.0, and combined 0.853. The small fractional gap
comes from unpublished raw substrings and is documented separately rather than tuned into the
product workflow.
Task 049 passed all 22 unchanged data-cleaning checks and exposed that independent review was still
a one-shot model completion without workspace tools. Review now runs through a fresh bounded D0
loop whose executor is read-only, requires a listing before targeted reads, and rejects undeclared
mutation or process calls. The Claude account route now carries the complete typed Peritus
host-tool catalog in its prompt and returns inert calls for Peritus to execute while Claude native
tools stay disabled. The final unchanged run began review with `workspace_list`, read all three
authoritative fixtures, reconciled all 25 rows, and completed natively with outcome 1.0, process
0.9867, security 1.0, and combined 0.9867 in 444.113 seconds.
Task 050 joined six financial tables into exact customer, region, audit, and reconciliation
artifacts. The first run passed all 26 unchanged oracle checks but exposed a live Claude response
shape in which a declared host call was embedded inside structured assistant content instead of
the outer call array. The account adapter now normalizes that reserved double envelope through the
same name, argument, and limit validator, while undeclared calls still fail closed; typed reviews
also require their explicit findings array. The unchanged rerun completed natively with outcome
1.0, process 0.9267, security 1.0, and combined 0.9267 in 388.938 seconds, reducing the request
count from 36 to 31. Both reports remain outside Git as
`reports/050-multitable-join-analysis-{pre,post}-embedded-tool-recovery.json`.
Task 051 completed cleanly on its first unchanged run. Native Peritus executed the supplied SQLite
database, independently checked the query result and audit identifiers, passed all 21 oracle
checks, and scored outcome 1.0, process 0.9533, security 1.0, and combined 0.9533 in 216.272
seconds.
Task 052 exposed three general review-convergence defects rather than a missing output capability:
advisory findings still blocked some material categories, a reviewer could circularly assume a
disputed trailing-modifier scope, and free-form location changes forked one stable finding into
multiple ledger identities. Advisory severity is now nonblocking, unresolved compound readings
are conserved unless authority settles them, named-category membership requires source support,
and finding identity uses normalized category plus stable title while refreshing location evidence.
The latest completed unchanged run passes all 17 oracle checks with outcome 1.0, process 0.8667,
security 1.0, and combined 0.8667 after recovering several provider stalls. That run predates the
final finding-identity correction and still reports native failure. A final unchanged rerun against
identity version 2 then completed natively with all 17 checks, outcome 1.0, process 0.93, security
1.0, and combined 0.93 in 272.484 seconds. It used 18 provider requests and 217,196 tokens. All
diagnostic reports remain outside Git under the external state directory's `reports/` folder.
Tasks 053 and 054 both completed natively on their first cycles. Task 053 passed all 29 anomalous-
transaction checks with outcome 1.0, process 0.96, security 1.0, and combined 0.96 in 554.279
seconds. Task 054 passed all 34 budget-variance checks with outcome 1.0, process 0.89, security 1.0,
and combined 0.89 in 608.309 seconds. Its lower process score records duplicate fixture reads, a
long planning pass, and stray post-completion text; because the independently recalculated output
was exact, these remain longitudinal efficiency observations rather than task-specific changes.
Task 055 initially passed all 24 funnel-analysis checks and completed natively, but took three
cycles, 48 provider requests, 808,266 tokens, and 653.52 seconds for process 0.76. Fresh fixers were
not told that repository-grounding credit resets every invocation, so they repeatedly attempted
patches before the enforced listing and target-read sequence; one also guessed the harness-owned
`peritus-internal` gate as a workspace executable. Tool descriptions and role prompts now state
those boundaries, and reviewers must reread the current files cited by conserved findings before
repeating them. The unchanged rerun completed natively in one cycle with zero rejected tool calls,
all 24 checks, process and combined 0.9867, 17 requests, 306,944 tokens, and 317.0 seconds.
Task 056 completed natively in one cycle with 24 of 25 checks and process 0.9533. Its sole oracle
failure contradicts the stated `more_than_one_pack_low_skus` rule: SKU-B's current stock is 50,
target is 21, and pack size is 5, so it is more than one pack above target, but hidden ground truth
requires omitting it. Task 057 preserved round-one state, processed only the two pending cases,
applied and audited patches, and completed natively across both rounds with process 0.92. Its two
oracle misses require an unpublished object shape for `per_item_results` and a hidden processing-
log step allowlist; the delivered array contains every correct score, and explicit
`round2_reused`/`skipped_preexisting` entries identify all three reused cases. Neither mismatch
caused benchmark-specific product behavior.
Task 058 initially timed out in Day 3 because its prompt required three file writes in one response
while the developer loop advertised a maximum batch size of one. The loop now projects the
provider's negotiated parallel-tool width, executes every returned call in stable proposal order,
and reports `changed: false` without rewriting when a full-file write already matches. Day 1 and
Day 2 each wrote their two artifacts in one batch; Day 3 wrote `project_log.md`, `final_plan.json`,
and `decision_register.csv` together. A further general cross-artifact fidelity rule preserved the
exact `conditional_go` identifier in the CSV audit. The final unchanged run completed natively in
one cycle per day with 10/11 checks, outcome 0.9375, process 0.9233, security 1.0, combined 0.8656,
34 requests, 493,235 tokens, and 694.419 seconds. The remaining oracle miss requires the literal
word `stakeholder` inside `final_plan.json` even though that file records the sales/privacy conflict
and `project_log.md` explicitly calls it a stakeholder conflict; no task-specific wording was added.
Task 059 completed both event-planning rounds natively. Its first run satisfied every scheduling
constraint but opened a future-round input early and omitted the literal `11:00` threshold from a
diff that described the unchanged rehearsal. The production workflow now isolates exact named
inputs by stage and requires revision reports to account explicitly for every introduced constraint,
including already-satisfied ones. The unchanged rerun did not read the update notice in round one,
passed all nine oracle checks, and scored outcome 1.0, process 0.9533, security 1.0, and combined
0.9533 in 369.801 seconds using 24 provider requests and 346,467 tokens.
Task 060 passed every cancellation and cleanup check on its first run, but the trace revealed that
`workspace_remove` could delete the owned draft file only, not the resulting empty `out/tmp`
directory. The developer attempted the exact deletion twice and then accurately reported the tool
limit. `workspace_remove` now supports one explicitly listed empty directory, non-recursively, while
continuing to reject the workspace root, nonempty directories, and unowned external files. The
unchanged rerun removed both the draft and directory through successful structured calls, passed all
seven oracle checks, and scored outcome 1.0, process 0.9367, security 1.0, and combined 0.9367 in
314.423 seconds using 25 provider requests and 355,717 tokens.

Task 061 initially exhausted its 180-second deadline because its mandatory generative design used
most of the run before the writer could perform a timed polling task. A token ceiling made the
document shorter but did not remove the provider's reasoning latency. Explicit artifact workspaces
now receive a detailed deterministic Rust design from the exact durable conversation and a bounded,
sorted inventory, while ordinary source repositories retain the generative architecture pass. The
first completed run then exposed two general semantics issues: a duplicate identifier leaked into
the separately defined ignored category, and one long sleep plus a final scan was treated as
periodic polling. The workflow now derives separately named categories independently and requires
at least three observations across a requested interval. The final unchanged run performed a
26-second multi-observation poll, preserved first-seen duplicate handling, passed all seven checks,
and scored outcome 1.0, process 0.9867, security 1.0, and combined 0.9867 in 167.85 seconds using
17 provider requests and 176,238 tokens.

Task 062 audited Kubernetes deployment, service, and policy fixtures in one native cycle, emitted
all eight required findings, preserved the inputs, and scored process/security 1.0/1.0. Its 0.9054
outcome reflects three severity labels from an unpublished taxonomy and two exact `missing` keyword
checks where the evidence says `has no`; no task-specific wording was added. Task 063 then separated
two topology-rooted production incidents from maintenance, heartbeat, and staging noise, passed all
checks, and scored outcome/process/security/combined 1.0/0.9233/1.0/0.9233.

Task 064 correctly identified `auth-gateway` change `AUTH-2026-0318`, the complete user-impact path,
five evidence sources, both misleading signals, and grounded mitigation and verification. Its
process score was 0.9967; the 0.8222 outcome retains an exact incident identifier that appears only
in hidden ground truth and one synonym check. Task 065 then exposed a real constraint-grounding
defect: the first plan assumed instance types without a `regions` field were available in the
required region. The embedded workflow now treats hard eligibility, compatibility, and placement
constraints as evidence-positive and refuses permissive defaults not declared by an authoritative
input. The unchanged rerun selected the only type with affirmative `us-east` support, passed the
capacity plan at 1.0, and improved outcome/process/security/combined from
0.6617/0.95/1.0/0.6286 to 0.9873/1.0/1.0/0.9873 while reducing runtime from 165.591 to 116.95 seconds.

Task 066 found all five real rollback blockers, issued the correct blocked recommendation, preserved
the incident-commander and database-owner approval boundaries, and made no execution claim. It
scored outcome/process/security/combined 0.8242/1.0/1.0/0.8242; the outcome deductions are an
unpublished severity taxonomy and exact keyword checks. Task 067 averaged every metric, applied both
delta directions correctly, handled missing and low-sample evidence, and chose rollback safely. Its
0.9263 outcome retains the unspecified normal `evidence_status`: Peritus used `sufficient`, while
hidden ground truth requires `ok`.

Task 068 passed every substantive product-launch planning check with perfect process/security. Its
single 0.08 oracle miss is a negation bug: explicit requirements not to promise mobile integration
contain the raw prohibited substring `promise mobile app`. Task 069 then passed all 17 legal-
compliance checks and scored 1.0 across outcome, process, security, and combined scoring. Task 070
ranked candidates by job evidence and applied the supplied threshold exactly. Its retained 0.70
outcome reflects hidden ground truth that rejects a candidate with three evidenced must-have skills,
contrary to the prompt, plus a raw `age` substring match inside `managers`.

Task 071 correctly routed all ten support tickets, including conservative fraud, dispute, conflict,
and VIP evidence handling, and scored process/security 1.0/1.0. Its 0.83 outcome retains ten
unpublished stable reply-key values and one hidden primary-clause preference. Task 072 correctly
joined every shipment, tier, delay, action, and customer message; its only miss searches for the
unpublished compensation token `shipping_credit` where the output states `$25 account credit`.

Task 073 mapped all four research claims and all four reproducibility gaps without claiming a rerun.
The pinned upstream tree contains no `in/scripts` directory or `analyze_main.py`, despite the prompt
and oracle describing a syntax-corrupted driver; its 0.82 outcome and process deduction are retained
as a fixture defect. Task 074 produced complete grading math and specific feedback with perfect
process/security; hidden ground truth contradicts the rubric's “at least two accurate details” rule
for submission 3. Task 075 passed every substantive moderation and privacy check and scored outcome
0.95/process 0.99/security 1.0; its sole miss is an overlapping calibration case that is both clear
counterspeech (`high`) and an advisory-automation conflict (typically `medium`).

Task 076 identified every present, missing, stale, mismatched, and conditionally required claim
document without medical judgment or identifier leakage. Its 0.83 outcome reflects adjacency-only
substring checks against semantically exact prose. Task 077 extracted all seven safe nested-archive
files with exact hashes and rejected all seven unsafe entries; its deductions require an unspecified
archive-chain delimiter and spaced prose equivalents of machine rejection keys.

Task 078 passed every local-API pagination, inventory, retry, checkpoint, and quality-report check,
scoring outcome/process/security/combined 1.0/0.93/1.0/0.93. Task 079 preserved and accounted for
all 45 batch inputs with exact successful and rejected sets, but hidden truth rejects source-native
identifier fields under a normalized-record schema that the prompt never publishes. Task 080
produced exact canonical JSON, YAML, and CSV with full type preservation and conflict resolution; its
sole miss is the unpublished conflict-source key name.

Task 081 produced the exact requested DOM extraction, excluded hidden template bait, and exercised
the detail, search, and confirmation routes with process/security 0.9933/1.0. Its only deduction
requires an HTTP request to `/` even though the benchmark supplies the exact index DOM as a local
authoritative input and Peritus read it before using the dynamic routes. Task 082 passed every
Compose contract and validation check with a 0.99 outcome. Task 083 passed all monorepo interface,
hidden behavior, test-integrity, quality, and report checks for outcome 1.0.

Task 084 passed JavaScript tests, hidden state behavior, and test integrity with perfect process and
security and a 0.9938 outcome. Task 085 removed the flaky race at its cause and passed eight repeated
test runs, hidden determinism, test integrity, implementation quality, and reporting for outcome
1.0. Task 086's migration, constraints, second execution, rollback, and postchecks all functioned;
its 0.70 cap comes from a hidden reason-string predicate that treats `invoice not found` as different
from `missing invoice` in both data and idempotence checks.

Task 087 initially passed all implementation and hidden CLI checks but omitted a direct descending-
sort regression. The production workflow, developer skill, and independent reviewer now map every
explicitly named regression behavior to a direct repository assertion. The unchanged rerun added the
missing ascending/descending numeric test, passed all nine tests and hidden behavior, and improved
outcome/process/security/combined from 0.8821/0.9767/1.0/0.8615 to
0.9107/0.9967/1.0/0.9077.

Task 088 initially normalized both API versions correctly and handled declared 429 and 400 responses,
but an endpoint-shaped base URL caused the server to repeat the same cursor and the client to loop
until the hidden check timed out. The production workflow and role skills now require external
pagination to prove bounded forward progress, reject repeated tokens, bound retry attempts, and
surface permanent errors immediately. The unchanged rerun used general v1 page and v2 cursor sets,
passed every visible and hidden check, and improved outcome/process/security/combined from
0.60/0.9867/1.0/0.5920 to 1.0/0.9667/1.0/0.9667.

Task 089's developer initially preserved the supplied experiment aggregates, but review guessed that
the separate exclusion ledger still had to be subtracted despite the absence of user-level metric
membership. The production workflow and role skills now forbid invented aggregate transformations:
schema semantics or a reconstructible record-level join must prove both inclusion and each metric
effect, while unresolved provenance remains advisory. The unchanged rerun passed all 18 outcome
checks in one review cycle and improved outcome/process/security/combined from
0.9524/0.8633/1.0/0.8222 to 1.0/0.9933/1.0/0.9933.

Task 090 initially found every anomaly and applied thresholds, severity, overlap priority, impact,
and low-volume suppression correctly, but bare event IDs lost their source category and summary
counts grouped by individual record. The workflow and role skills now preserve typed identities for
references spanning heterogeneous sources and group semantic category summaries by category. The
unchanged rerun passed all 16 outcome checks in one review cycle and improved
outcome/process/security/combined from 0.5424/0.96/1.0/0.5207 to
1.0/0.9633/1.0/0.9633.

Task 091 computed every FX conversion, refund, bank fee, synthetic cash row, and summary amount
exactly, but initially flattened material statuses, duplicated dedicated exception rows into the
reject ledger, used an overly broad invalid-reference reason, and counted unresolved rows from only
one artifact. Reconciliation guidance now routes identities once unless overlap is explicit,
preserves material state, distinguishes absent from present-but-invalid references, and reconciles
summary exceptions across all outputs. The unchanged final run passed all 20 outcome checks and
improved outcome/process/security/combined from 0.7705/0.9767/1.0/0.7525 to
1.0/0.9633/1.0/0.9633.

Task 092 reported all six required drift identities and all five rejected rows with complete
evidence. Its retained 0.74 outcome reflects an unpublished severity map, unpublished precedence for
a row with two valid defects, and an unspecified mismatch-summary shape. The process rubric also
penalizes value failures appearing in the drift report even though the deterministic oracle requires
those exact value-level drift categories, so no contradictory benchmark-specific rule was added.

Task 093 correctly stitched identities, sorted events, enforced the exact 30-minute boundary,
deduplicated event IDs, excluded bots, and produced the exact summary. Its retained 0.5429 outcome
comes from unpublished cross-session campaign carryover, unpublished duplication of an excluded bot
into both summary and reject ledger, and a raw hyphenated keyword check against equivalent prose.

Task 094 initially mixed a `requires_review` metric into a regression-only ledger. The workflow and
role skills now treat a detail ledger as a projection of its named closed class unless overlap is
explicit. A subsequent run exposed a separate real harness defect: after the native CSV gate found a
ragged row, the reviewer produced the right finding but exhausted fresh grounding retries before the
fixer ran. The developer loop now continues an early terminal inside the same tool session and
preserves partial grounding evidence. The unchanged final run completed with every native gate
passing and process/security 0.9967/1.0. Its retained 0.78 outcome reflects unpublished direction
defaults, caveat substring checks, and output variability that reintroduced the review row; the
general category and recovery rules remain in force.

Task 095 initially named a partial policy as applicable authority for a true
`insufficient_evidence` result and omitted several evaluated losing sources. The workflow now
preserves explicit empty/null authority sentinels, accounts for losing rules across priority, date,
expiry, scope, and exceptions, and keeps exact source identities separate from explanatory reason
text. The unchanged final run made all 11 decisions, governing sources, evidence signals,
insufficient-evidence cases, and forbidden outcomes correct; conflict provenance and the conflict
audit passed, process/security were 1.0/1.0, and the retained 0.74 outcome reflects unpublished
scope-token spellings rather than a missing policy decision.

Task 096 correctly classified and sourced every offline knowledge answer, including partial and
insufficient-evidence cases, but initially repeated an archived unapproved phone placeholder while
rejecting it. The workflow now cites the source and reason without unnecessarily reproducing stale,
draft, superseded, unapproved, or unsafe actionable literals. The unchanged rerun preserved all 14
statuses and sources, eliminated the stale value, earned perfect process and security scores, and
improved outcome/combined from 0.65/0.6457 to 0.9586/0.9586 (`excellent`).

Task 097 classified all 14 research claims correctly, retained every required secondary source,
identified all missing reproducibility prerequisites, and produced a perfect-status evidence
matrix. The workflow now requires independently resolvable location fields and exact missing paths
in every artifact that records `not_reproducible`, rather than relying on signal prose or a richer
sibling output. The unchanged final run retained outcome 0.72: exact claim decisions,
reproducibility gaps, and scope limits passed, while generative location placement and unpublished
preferred source or phrase choices remained variable.

HarnessBench chooses a result directory from the last observed provider model and may move a
multi-provider sandbox after the native adapter exits. Invocation evidence schema 4 therefore
includes `relocatable_paths`, all rooted at the final sandbox printed by HarnessBench. Those paths
continue to resolve after the move even though the upstream report's earlier absolute
`usage_summary.log_file` value still names its pre-move location.
Product failures and benchmark defects are kept separate in the
[external failure journal](../failure-journal.md); generated workspaces, native traces, and full
result JSON remain in the configured external state directory rather than Git.
