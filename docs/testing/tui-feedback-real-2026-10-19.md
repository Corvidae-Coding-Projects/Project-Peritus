# TUI activity and steering feedback

The user reported that active work hides steering receipts and file changes, while plain,
unstructured output makes long runs hard to understand. The source comparison identified
the active footer shortcut, generic file-tool JSON, and literal Markdown rendering.

## Observable contract, defined before replay

- During actual running work, show saved and incorporated input revisions independently.
  Sending a steering instruction must not imply it is already in a model request.
- A successful file patch shows its exact path, first affected line, and removed/added fragment.
  Failed patches and failed commands remain visibly failed and never become a successful edit.
  Full-file writes show the contents written without claiming a before/after diff.
- Show the current observed operation and last successful file change above the composer.
- Format assistant headings, emphasis, lists, code, links, and tables as terminal text/styles.
  Terminal controls stay inert; Markdown never fetches linked images or pages.
- Preserve drafts, receipt visibility, output selection, details, and ordinary reconnect/reattach
  through terminal resizing. Reopening a completed run starts no inference.
- Keep formatting caches within the retained activity window and avoid reparsing unchanged
  messages on every UI tick.

Replay uses the actual packaged CLI/TUI, local daemon, signed-in account provider, real tools,
and a disposable Python CLI. It deliberately rejects one patch, applies the requested patch and
file write, steers while a real command waits, then checks the executable's output independently.
Actual terminal cell styles and screens are retained outside the repository. No unit tests,
mock providers, formal verification, or repository qualification suites are used.

Drivers: [live tools and steering](reproducers/tui-feedback.py) and
[retained-run terminal replay](reproducers/tui-feedback-view.py).

## Results

The installed 0.1.1 client reproduced hidden steering receipts during active work and
unformatted output. The replacement used the real signed-in provider and actual file and
command tools in disposable workspaces. Observed results:

| Journey | Independent observation |
| --- | --- |
| Steering during a running Python process | The screen first showed saved r2 / in model request r1, then r2 / r2. |
| Rejected patch followed by a valid patch | The rejected patch remained failed. The successful edit showed `greet.py:2` with removed `Before` and added `After café 雪`. |
| Full-file write | `notes.md` contained exactly `FEEDBACK-NOTE`; the display labeled the contents as a write preview. |
| Real execution | A fresh `python3 greet.py` printed `After café 雪`. The intentional exit-7 command was visibly failed. |
| Reports | A real provider response displayed a styled heading, emphasis, table, code block, and link. Actual terminal cells retained heading color and bold styling. |
| Details | Routine JSON was compact; `/details` exposed the retained original read result, including the independently calculated source hash. |
| Resizing and reconnecting | Thirteen dimension transitions, including 36×14 and 160×48, preserved the draft and steering receipts. Ctrl-R reconnected without submitting the draft. |
| Recovery display | Cancelling only the disposable in-place work retained effects as RecoveryRequired; steering receipts stayed visible with the recovery hint. |

The live replay also found that completed role responses stayed as JSON until a candidate
checkpoint existed. Projection now formats recognized completed responses while review or
fixing is still running. The raw response remains available in retained details and the
underlying run history is unchanged.

The terminal replay explicitly removes the controller's `NO_COLOR` environment variable to
exercise a color terminal. An earlier color assertion failed because that variable was
inherited; the application correctly honors it. Other driver corrections used the actual
`source_sha256` read-result field and accepted RecoveryRequired as a terminal cancellation
outcome. These were oracle corrections, not additional product failures.

## Reproduction and retained evidence

Build the three actual binaries with bounded parallelism:

```sh
CARGO_TARGET_DIR=/home/doll/.cache/peritus-tui-feedback-target \
  CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked --jobs 2 \
  -p peritus-cli -p peritus-daemon -p peritus-tui
```

The live driver requires an existing provider-selection state file and signed-in account
credentials. Its `--output` must be a new disposable directory; no user project is edited.
Use `uv run --with pyte python3` to supply its terminal decoder. The retained-run driver
requires no additional inference:

```sh
uv run --offline --with pyte python3 \
  docs/testing/reproducers/tui-feedback-view.py \
  --binary /home/doll/.cache/peritus-tui-feedback-target/debug/peritus \
  --instance /tmp/p-tui-final-live \
  --output /tmp/peritus-tui-feedback-evidence/final-verified-view
```

Evidence is outside the repository:

- `/tmp/peritus-tui-feedback-evidence/baseline/`: original installed-client observation.
- `/tmp/peritus-tui-feedback-evidence/final-live/`: actual live action screens, terminal
  bytes, and timestamped frames, including both receipt states, patch fragments, and exit 7.
- `/tmp/p-tui-final-live/`: isolated project, durable real-provider run, and terminal evidence.
- `/tmp/peritus-tui-feedback-evidence/final/frontend-before-projection-fix/`: raw role-envelope
  presentation before the projection correction.
- `/tmp/peritus-tui-feedback-evidence/final-verified-view/`: final development-binary PASS
  result, actual terminal cells/styles, original details, 13 sizes, and reconnected draft.
- `/tmp/peritus-tui-feedback-final-verified-view.log`: final replay output.

The final development launcher SHA-256 was
`75a240926acdd3461824a997cf8cc40d594d208420d96a786a0e28e6e23c2135`.
The full live driver did not produce an overall PASS marker before its terminal-state/color
oracle corrections; its completed operations are retained individually. The final retained-run
driver passed all of its checks on the final code. No fresh provider run was needed for those
presentation-only corrections.

Coverage is the Linux terminal product path with the OpenAI account provider. This is not
exhaustive platform/provider coverage, a leak assessment, or candidate qualification. No
unit tests, mocked providers, formal verification, or repository qualification suites ran.

## Installed release build

An optimized build of the same final code completed with `cargo build --locked --release
--jobs 2 -p peritus-cli -p peritus-daemon -p peritus-tui`. The old three binaries were backed
up under `/home/doll/.local/state/peritus/binary-backups/tui-feedback-20261019T152152Z`.
No personal daemon was running during replacement. The rebuilt client, TUI, and daemon
were installed in `/home/doll/.local/bin`; the launcher and daemon report version 0.1.2.

The retained-run driver then passed through the **installed** launcher and TUI, with the
installed daemon confirmed through `/proc/<pid>/exe`. All 13 size transitions, original
details, Markdown styles, reconnect, and retained-draft checks passed again. Model request
count stayed at 10: reattaching and viewing started no inference. The disposable daemon
was gracefully shut down afterward.

| Installed binary | SHA-256 |
| --- | --- |
| `peritus` | `b17607c1ee4fbcf0d2d6b261845ebadfa362c79be3b2d73e2e8605c823dbc871` |
| `peritus-tui` | `c745476db033873a2d44d6a562fbac222d1dba4997689363991fcf7f85250a8e` |
| `peritusd` | `4e884855d06548e4535b49477596c22aec3a8647f8d2d3166246cbac015614d1` |

Installation manifest: `/tmp/peritus-tui-feedback-evidence/installation.json`.
Installed replay: `/tmp/peritus-tui-feedback-evidence/installed-release-view/` and
`/tmp/peritus-tui-feedback-installed-release-view.log`.

The same three optimized binaries were subsequently installed for `builder`, with builder
ownership and matching hashes. Its idle old daemon was shut down gracefully; the ordinary
launcher opened the retained cancelled session through a real terminal and showed both
steering receipts. The new daemon executable/hash was independently confirmed, and the
session remained cancelled. Backup:
`/home/builder/.local/state/peritus/binary-backups/tui-feedback-20261019T152716Z`.
Private replay capture: `/tmp/peritus-builder-tui-upgrade-20261019/`.
