# HarnessBench

HarnessBench tests whether Peritus's orchestration, recovery, verification, state handling, and
tool-use policies improve real agent work. Peritus runs the pinned, unchanged suite through its
normal design, writer, reviewer, fixer, and deterministic-gate pipeline.

The pin in `pin.toml` identifies Qihoo360 HarnessBench commit
`1025086a446653702b80cfb48babbeec35db6b2c`, which contains 106 tasks. Upstream source, generated
workspaces, credentials, traces, and full results stay outside this repository.

## Prepare the runner

Build the native adapter with a bounded Cargo job count:

```sh
git diff --quiet HEAD --
PERITUS_SOURCE_REVISION="$(git rev-parse --verify HEAD)" \
  CARGO_BUILD_JOBS=2 cargo build --locked \
  --package peritus-external-benchmarks \
  --bin peritus-benchmark-agent
```

The first command refuses tracked source changes. The compiled revision and the executable's
runtime SHA-256 are retained in every schema-version-5 invocation report.

Clone or reset HarnessBench to the pinned commit. Create a virtual environment outside the
repository so the suite and its unchanged task oracles use the same dependencies:

```sh
python3 -m venv /absolute/path/to/benchmark-state/.venv
/absolute/path/to/benchmark-state/.venv/bin/python -m pip install \
  --requirement /absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/oracle-requirements.txt
```

Copy `app.example.json` to a local file outside Git and replace its result and workspace paths.
List the pinned tasks from the HarnessBench checkout:

```sh
PYTHONPATH=src /absolute/path/to/benchmark-state/.venv/bin/python \
  -m harnessbench.cli tasks
```

HarnessBench expects an OpenAI-compatible endpoint for its process rubric. If a compatible rubric
credential is not configured, start Peritus's localhost bridge in another terminal:

```sh
python3 /absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/rubric_server.py \
  --agent /absolute/path/to/Project-Peritus/target/debug/peritus-benchmark-agent \
  --port 8765
```

The bridge forwards bounded rubric requests to the native Rust adapter, which uses the logged-in
official `codex` executable as a model router. The bridge does not read or copy account credentials.

## Run an unchanged task

From the HarnessBench checkout:

```sh
PATH=/absolute/path/to/benchmark-state/.venv/bin:/absolute/path/to/Project-Peritus/target/debug:$PATH \
HARNESSBENCH_APP_CONFIG=/absolute/path/to/local-app.json \
HARNESSBENCH_HARNESS_CONFIG=/absolute/path/to/Project-Peritus/benchmarks/external/harnessbench/harness.json \
HARNESSBENCH_PUBLIC_URL_TEMPLATE='{local_url}' \
RUBRIC_API_KEY=peritus-local-rubric \
RUBRIC_BASE_URL=http://127.0.0.1:8765/v1 \
RUBRIC_MODEL=gpt-5.6-sol \
PYTHONPATH=src /absolute/path/to/benchmark-state/.venv/bin/python \
  -m harnessbench.cli run-task \
  --task 001-file \
  --harness peritus-codex-claude \
  --mode live
```

`HARNESSBENCH_PUBLIC_URL_TEMPLATE='{local_url}'` allows tasks 003 and 006 to use their local fixture
servers. It does not replace or bypass the task server.

HarnessBench owns task setup, deadlines, workspaces, oracles, process rubrics, and scoring. Peritus
may initialize Git in a supplied workspace that has no history, but it must not edit task fixtures,
hooks, rubrics, or oracles. Do not set `HARNESSBENCH_SKIP_PROCESS_GRADE` during a scored run.

## Completed diagnostic baseline

The completed baseline contains all 106 tasks with no missing or failed native adapter run:

| Measure | Result |
| --- | ---: |
| Mean outcome | 0.8969 |
| Mean process | 0.9286 |
| Mean security | 1.0000 |
| Mean combined | 0.8331 |
| Perfect-outcome tasks | 40 |
| Tasks with outcome at least 0.9 | 64 |
| Total execution time | 8.529 hours |
| Total model tokens | 31,286,948 |

The campaign led to broad product improvements in recovery, durable multi-turn state, deterministic
format gates, independent review, evidence handoff, exact-identifier preservation, bounded tool
parallelism, constraint grounding, pagination, polling, and artifact reconciliation. It did not add
task-specific answers or weaken validation to raise the score.

This aggregate was collected while those general fixes were being implemented, so its task reports
bind different development checkpoints. It is the honest diagnostic baseline, not the frozen final
candidate result. A second complete run with one schema-version-5 binary built from the exact final
commit remains required for the final comparison.

Every reproduced product defect, upstream defect, retained mismatch, before/after run, and general
fix is recorded in the [external failure journal](../failure-journal.md). That journal is the place
for task-level detail; this guide stays focused on setup, operation, and the final result.

HarnessBench may move a multi-provider sandbox after the adapter exits. Invocation evidence schema
4 therefore includes `relocatable_paths`, all rooted at the final sandbox printed by HarnessBench.
Use those paths when locating retained external evidence.
