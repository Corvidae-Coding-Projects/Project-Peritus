# Open item replays

These consumers operate actual daemon IPC, native terminal rendering and public application APIs.
`readiness.py` uses an absent session D-Bus, inert credential references, isolated application
directories and real `peritusd`; it never impersonates an AI service or reads user credentials.

Build `peritus-cli`, `peritus-daemon`, and `peritus-web` with one job, debug symbols disabled,
and `--message-format=json`. Give `link.py` that complete Cargo log to link `readiness.rs`
and `ui.rs` to the exact same artifact cohort. Seed the fixture with the public
[configuration-upgrade consumer](../configuration-upgrade/README.md).

```sh
python3 docs/testing/reproducers/open-issues/link.py --build BUILD.jsonl \
  --source docs/testing/reproducers/open-issues/readiness.rs --output /path/to/readiness
python3 docs/testing/reproducers/open-issues/link.py --build BUILD.jsonl \
  --source docs/testing/reproducers/open-issues/ui.rs --output /path/to/ui
uv run --with pyte python3 docs/testing/reproducers/open-issues/readiness.py \
  --binary-dir /path/to/current/binaries --seed-consumer /path/to/configuration-upgrade-consumer \
  --probe /path/to/readiness --ui /path/to/ui --fixed --output /path/to/evidence
```

For the released baseline, point `--binary-dir` at the unpacked v0.1.9 native package and omit
`--fixed` and `--ui`. The independent oracle requires `not-ready` with recovery guidance on the
fixed program, continued revision-fenced editing after rejection, the original credential cause
on native TUI retry, and usable model recovery controls. Earlier binaries report
`invalid-identifier` instead. Missing requested protocol features correctly reject the driver
before admission; the consumer explicitly negotiates workbench control, inputs and execution.

The native terminal path currently uses Linux/Unix PTYs; browser setup downloads are disabled
inside its disposable processes. The readiness replay does not claim provider inference, Windows/macOS
execution or the separate macOS real-testing issue.

`provider-switch.py` exercises retained-run provider changes and retries through the real official
Codex account. `model-key-reuse.py` checks model-only settings without key entry. `healing.rs` and
`tool-output.rs` consume actual public library interfaces with varied inputs; their results do
not establish paid-provider inference.

`acceptance-contract.rs` reproduces issue #155 through the actual public `peritus-gates` API. Link
it from a current application build, then give it an absent or empty disposable fixture directory:

```sh
python3 docs/testing/reproducers/open-issues/link.py --build BUILD.jsonl \
  --source docs/testing/reproducers/open-issues/acceptance-contract.rs \
  --output /path/to/acceptance-contract
/path/to/acceptance-contract /tmp/peritus-issue155-fixture
```

The consumer creates a Make-owned modpack-data workspace, requires complete coverage with no
uncovered paths, proves that `make check` rather than the cheaper `check-fast` target was selected,
reads every changed file, and executes the selected full check. It never runs a default or unknown
Make target.

Issue #159's `pipeline-handoff.rs` operates the actual ProductRunner with official provider
inference and disposable log/lock fixtures. It uses the existing official Codex account by default;
`--zen` selects the actual OpenCode Zen/DeepSeek transport and reads the explicitly selected
`PERITUS_REAL_KEY_FILE`. It consumes provider usage. Build and run it outside the checkout:

```sh
python3 docs/testing/reproducers/open-issues/build-pipeline-handoff.py /absolute/new/consumer
/absolute/new/consumer/target/debug/peritus-pipeline-handoff-replay \
  /absolute/new/approved-case docs/testing/reproducers/open-issues/pipeline-authorized.txt
/absolute/new/consumer/target/debug/peritus-pipeline-handoff-replay \
  /absolute/new/readonly-case docs/testing/reproducers/open-issues/pipeline-readonly.txt
```

The approved case must enter the pipeline and produce a matching `identity-report.md`; the
read-only case must answer without entering design or writing a report. Both must preserve the
input files and `keep.txt`. `--plan` selects read-only Plan mode. With `--zen`, a private transparent
vendor capture wrapper can be selected through `PERITUS_REAL_CAPTURE` and the existing capture
driver's environment; it must relay unchanged official vendor input/output, never replace results.
See [the campaign report](../../pipeline-handoff-real-2026-10-26.md) for the observed SQLite
storage failure, real runtime-directory recovery, and coverage limits.

`pipeline-promise.rs` is a narrower component replay which compiles the exact host reply check
and reads one proposed public reply from stdin. A `true` result means correction is required
before a promise can finish the conversation. It does not launch a pipeline and does not by itself
establish successful model correction. Build it with `rustc --edition=2024` and pass the screenshot's
promise, quotations/code, conditional offers, and informational explanations through stdin.

`context-display.py` performs a real-account source read and archived-source retrieval, retaining
an isolated fixture JSON. Pass that fixture to `read-preview.py --fixture FIXTURE.json` along
with `--binary-dir`, `--ui`, `--probe` (the linked `selection.rs`) and `--output`. Before the fix,
omit `--fixed`; after it, add `--fixed` to operate compact/expanded cards at 80, 120 and 160 columns.
`--variants` additionally requests real Markdown, Unicode, short-file and missing-file reads.
`--variants --variants-only` reopens those recorded variant results without requesting inference
again. Retain the native ANSI/screen artifacts and result JSON outside the checkout.
