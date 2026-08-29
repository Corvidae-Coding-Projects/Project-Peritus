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

The live baseline has exercised tasks 001 through 047 against the pinned, unchanged suite. Tasks
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
Product failures and benchmark defects are kept separate in the
[external failure journal](../failure-journal.md); generated workspaces, native traces, and full
result JSON remain in the configured external state directory rather than Git.
