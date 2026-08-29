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

The live baseline has exercised tasks 001 through 057 against the pinned, unchanged suite. Tasks
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

HarnessBench chooses a result directory from the last observed provider model and may move a
multi-provider sandbox after the native adapter exits. Invocation evidence schema 4 therefore
includes `relocatable_paths`, all rooted at the final sandbox printed by HarnessBench. Those paths
continue to resolve after the move even though the upstream report's earlier absolute
`usage_summary.log_file` value still names its pre-move location.
Product failures and benchmark defects are kept separate in the
[external failure journal](../failure-journal.md); generated workspaces, native traces, and full
result JSON remain in the configured external state directory rather than Git.
