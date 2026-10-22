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

`context-display.py` performs a real-account source read and archived-source retrieval, retaining
an isolated fixture JSON. Pass that fixture to `read-preview.py --fixture FIXTURE.json` along
with `--binary-dir`, `--ui`, `--probe` (the linked `selection.rs`) and `--output`. Before the fix,
omit `--fixed`; after it, add `--fixed` to operate compact/expanded cards at 80, 120 and 160 columns.
`--variants` additionally requests real Markdown, Unicode, short-file and missing-file reads.
`--variants --variants-only` reopens those recorded variant results without requesting inference
again. Retain the native ANSI/screen artifacts and result JSON outside the checkout.
