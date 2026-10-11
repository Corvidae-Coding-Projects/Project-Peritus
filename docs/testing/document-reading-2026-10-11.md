# Larger document reading: real campaign and defects

Baseline: develop `e958073fdf88a07366e0a9ea4a23f744b2836020`. Fedora Linux, real Peritus
CLI/TUI/daemon and account-backed Codex 0.162.1 with gpt-5.6-sol. No unit, formal,
repository qualification or Clippy suites were run. Ordinary generated document data and
independent marker oracles live outside the repository under
`/home/doll/Peritus-large-document-reading-2026-10-11/`.

## Changed behavior

- `workspace_read` defaults to a 1 MiB encoded response budget, with no implicit 500-line cutoff.
- Its maximum derives from the existing JSON protocol capacity, currently 16 MiB, instead of
  imposing a separate 512 KiB file-read ceiling. Explicit line ranges and smaller budgets work.
- Workspace, attachment and immutable review-evidence read pages reach model context intact;
  they do not receive a second generic 30,000-byte head/tail projection.
- Empty lines consume the read budget. Response fitting uses a binary prefix search to avoid
  repeatedly serializing a large document once per omitted line. Long UTF-8 lines retain
  exact byte continuation offsets.
- Codex's structured upstream input-size rejection is classified explicitly, with an actionable
  size explanation rather than a network/authentication suggestion. Raw stderr is not forwarded.

## Confirmed defects

### DR-001 — A complete file read lost its middle before reaching the model

Request one `workspace_read` of the 44,004-byte `workspace-baseline/design.md` with
`max_bytes=131072`, then return three independent first/middle/last markers. The actual old
host observation contained the complete file, but the D0 projection removed its middle.
Run `b90281d52694202f27c802d49be09fc0` returned the correct first/last markers and null for
the middle, explicitly reporting truncation. Expected: all three markers in one read.

Repair locations: [document reader](../../crates/app/peritus-product-runner/src/developer_tools/inspection_read.rs),
[tool schema](../../crates/app/peritus-product-runner/src/developer_tools/catalog.rs),
[model-visible output policy](../../crates/orchestration/peritus-agent/src/developer/observation.rs).
The small default page and implicit line cutoff were also increased as requested.
Final exact replay `ef93ed4d558de777c18649b2d365b0c4` returned all three original markers
from the unchanged 44,004-byte file in one read (three provider requests, two tools).

### DR-002 — Codex input-size rejection appeared as a provider disconnection

The repaired reader returned all 2,771,652 bytes and 18,000 lines in a requested 4 MiB page.
Run `2f04467f482222ed93b65797f9012666` then stopped with `openai.codex_runtime.interrupted`
and advice to check network access. The independent official CLI reproduction reported:
`input_too_large`, `max_chars=1048576`, `actual_chars=2771806`.

Cause: the native CLI emitted its structured turn/start rejection on stderr and only a
thread-start event on stdout. Peritus treated that incomplete JSONL lifecycle as transport loss.
Repair: inspect only the structured diagnostic code on a failed process, classify input_limit
as an invalid request/context-capacity cause, and provide a size-specific hint.

Repair locations: [native failure classification](../../crates/model/peritus-provider-openai/src/runtime/output/failure.rs),
[process outcome handling](../../crates/model/peritus-provider-openai/src/runtime/provider.rs),
[recovery classification](../../crates/model/peritus-provider-core/src/recovery.rs),
[user-facing explanation](../../crates/orchestration/peritus-agent/src/developer/error.rs).
Final run `635289f69051b8a0ac48b8204157b31d` retains the complete host read and reports
`openai.codex_runtime.input_limit` with guidance to read smaller pages or select another provider.
The independent replay checker passes. Reading this entire document into the selected Codex
runtime in one turn remains **BLOCKED by that upstream limit**, not counted as a model-read pass.

## Real results

Seed `20261011`. The checker parses actual Peritus trace frames, compares returned text with
source files, and checks the model's marker receipts against an oracle outside its workspace.
The fixtures are ordinary files; no provider, reader, UI or response implementation is substituted.

| Scenario | Result and evidence |
| --- | --- |
| 672-byte Unicode/quoted document, default read | PASS, all source lines and model markers |
| 44,774-byte design document, default read | PASS, one complete read with all three markers |
| 255,730-byte / 2,000-line document, default read | PASS, includes lines beyond 500 and exact middle/last markers |
| 238,116-byte escaped Latin/Arabic/emoji single line | PASS, 4 KiB page continued at byte 3446; two returned chunks reconstruct the exact source |
| First 4 KiB page from two million blank lines | PASS, bounded response ends at line 583 and resumes at 584 |
| 769,652-byte / 5,000-line document, default read | PASS, one read; real model returns every independent marker and reaches line 5000 |
| Explicit lines 1490–1510 | PASS, exact 21-line inclusive range and matching metadata |
| Empty document | PASS, empty content without an invented continuation |
| Actual invalid UTF-8 file | PASS, explicit rejection |
| 2.77 MB host read and upstream rejection | PASS for complete host read and correct rejection classification; model consumption BLOCKED upstream |

Ordinary run `f3d888cc791d743d7eddfaceac1c95fc`: four provider requests, seven tools.
Final large-document run `7b26a36c1d6ef0f64970be44604822b9`: three requests, two tools.
Range/error run `35379f312be8fdbb252326990cd2d8c9`: two requests, four tools.
Final input-limit run: three requests, two tools, no retry loop.
Read-only completions use phase 11 (ordinary idle); expected upstream rejection uses phase 8.

## Build, replay and resources

```sh
CARGO_BUILD_JOBS=2 cargo build --release --offline --locked --bins --package peritus-cli --package peritus-daemon --package peritus-tui --package peritus-sandbox-linux
python3 docs/testing/reproducers/document-reading/prepare-fixtures.py /absolute/disposable/campaign
```

Open the generated workspace with the built Peritus. Type `/plan` and Enter, then paste a
retained task from the campaign root as an ordinary message. Use a new conversation for each
task. Do not bracket-paste the slash command itself: the TUI deliberately rejects pasted commands.
Inspect its actual saved run and trace with `check-replay.py FIXTURE RUN_JSON TRACE --mode
ordinary|large|ranges|input-limit`. The trace source path is recorded in build-origin files.

Final daemon SHA-256: `3dc8b6b7b9ebf0b42c00a14082b92a5448213487fa0897d7c0f17ad2d654353b`.
The final release build completed in 2m16s; build/source identities are in
`build-origin-final.json`. The earlier ordinary read campaign used `build-origin.json`;
its reader/output-policy source is unchanged in the final build.

Builds and fixtures ran sequentially in user systemd scopes: four cores, lowered priority,
8 GiB maximum build memory / 4 GiB fixture memory. The final fixture scope peaked at
195,784,704 bytes (about 187 MiB), with no high/max/OOM events. All fixture scopes and
owned daemons were shut down cleanly. The user's existing daemon was preserved.

The 16 MiB protocol envelope is not a promise that every provider accepts that much input.
Codex's current native CLI guard applies to the complete framed turn, including retained
conversation, at 1,048,576 characters. Other provider paths and the full 16 MiB boundary
were not exercised in this campaign. No exhaustive document or application coverage is claimed.
