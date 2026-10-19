# TUI conveniences — real interface evidence, October 19, 2026

## Contract and scope

Implement local transcript search, keyboard queue controls, fuzzy file selection, explicit clipboard
image paste, external draft editing, answer/code copying, configurable convenience shortcuts, and
syntax themes. Opening or using these controls must not initiate inference, erase an unrelated
composer draft, execute pasted commands, reinterpret a stale selected input, or attach a file/image
without the existing explicit confirmation. Settings and captured transcript history survive restart.

The requirements and action expectations are encoded in the replay driver before execution.
No unit tests, formal verification, repository verification suite, or simulated provider was used.
The implementation is confined to the TUI; daemon authority and provider routing are unchanged.

## Build and actual application

```sh
CARGO_TARGET_DIR=/home/doll/.cache/peritus-tui-feedback-target CARGO_PROFILE_DEV_DEBUG=0 \
  cargo build --locked --offline --package peritus-cli --package peritus-tui --jobs 1
```

The actual launcher and TUI ran against a compatible packaged daemon in disposable XDG state,
with a committed disposable Git project and private managed worktree. A real OpenAI account
produced the read-only response used for transcript, copy, and syntax checks: it read notes.md
through workspace_read and returned a Rust code block. Later replays reopened that same stopped
run instead of requesting more provider output. Clipboard checks used real Xvfb/xclip and a
2×2 PNG in an isolated display; the desktop clipboard was not changed. The external editor was
an executable fixture with spaces in its path that edited the application's real temporary draft.

## Observed results

| Journey | Observation | Result |
| --- | --- | --- |
| File discovery | Fuzzy Unicode filename match; ignored build file absent; project-relative readable labels; explicit absolute @ path preserved | PASS |
| First attachment | Both bare @ and Ctrl+O create their first conversation and reach immutable byte preview; normal draft survives | PASS |
| Clipboard | Actual clipboard PNG reaches daemon-validated image preview; original draft retained; no automatic attachment/inference | PASS |
| External editor | Quoted executable path, Unicode multiline round-trip, terminal restoration, and missing editor recovery retain an unsent draft | PASS |
| Queue | Exact selected input edited, held/released, moved, withdrawal canceled and confirmed through daemon receipts; main draft preserved | PASS |
| Shortcuts | New binding works immediately; reserved stop key rejected; reset works | PASS |
| Themes | Saved selection survives client restart; real Rust output has distinct syntax colors in light/dark and plain foreground in plain mode | PASS |
| Copy | Native clipboard readback contains the actual answer or selected code, excluding Markdown fence syntax | PASS |
| Transcript | Tool-result text found through Ctrl+F; complete result opened; same history searchable after a fresh client reattachment | PASS |
| No inference | Provider request count unchanged across local controls and reattachment | PASS |
| Generated workflows | Seed 20261019; 12 Unicode/punctuation drafts × three shuffled local overlays; all 36 returns retain the exact draft | PASS |

Optimized local journey: `/home/doll/.cache/peritus-tui-conveniences-20261019-optimized-local/results.json`
(56 checks including the 36 generated checks and explicit absolute-path compatibility).
Optimized retained-response journey:
`/home/doll/.cache/peritus-tui-conveniences-20261019-optimized-response/results.json`
(47 checks including the same 36 generated checks). These are overlapping journeys, not 103
unique feature scenarios. ANSI streams and decoded screens are beside those records.

The production transcript module also passed five separate consumer processes with a 300-activity
corpus: actual ripgrep and absent-ripgrep fallback, all public activity kinds, Unicode case folding,
literal regex characters and leading options, multiline/NUL searches, exact large/control-bearing
bodies, unchanged-write mtimes, same-sequence replacement, run isolation, symlink rejection,
private modes, truthful 200-hit truncation, and fresh-process restart readback. Final replay record:
`/home/doll/.cache/peritus-tui-conveniences-archive-final-v2/results.json`.

## Confirmed defects discovered and corrected

1. **First-conversation file selection erased an unrelated draft.** Ctrl+O from a new conversation
   with `KEEP-MY-DRAFT`, select the Unicode Rust file, and observe the composer after creation.
   The earlier build displayed a blank composer. Conversation reset dropped the local attachment
   intent. Carry that intent through creation and retain the draft through the deferred preview.
   Captures: `...-third/08-new-conversation-file-draft.screen` (failure), `...-acceptance/08-new-conversation-file-draft.screen` (pass).
2. **Automatic preview could erase that same draft later.** The deferred read was incorrectly
   registered as a consumable composer command, so its snapshot reply cleared the original text.
   Local attachment previews now bypass command-draft consumption. Capture: `...-sixth/08-new-conversation-file-draft.screen` (failure); acceptance journey verifies the complete preview (pass).
3. **A previous file panel hid the image panel.** Paste a clipboard image after closing a file
   preview. The image operation ran while the old file-open flag still won rendering priority.
   Image and queue entry now explicitly release file-panel ownership. Capture: `...-fourth/final.screen` (failure), `...-acceptance/10-image-clipboard-preview.screen` (pass).
4. **Absolute workspace prefixes clipped filenames.** With a long disposable state path, the
   file picker hid the filename extension even at 140 columns. Results now show project-relative
   names while retaining absolute paths for exact reads. Capture: `...-final-local/final.screen`
   (failure), acceptance fuzzy selection (pass).

5. **Image paste after reattachment stopped before preview.** Reopen a retained run, retain an
   ordinary unsent draft, and paste a ready isolated clipboard PNG. The old code refreshed absent
   workbench metadata by replaying that ordinary draft as a slash command; it silently abandoned
   the image read. Image preview now has an explicit scope/path-bound continuation through a fresh
   metadata snapshot, also used by manual preview, and cancellation abandons that continuation.
   `...-attachment-confirm-v2/final.screen` preserves the failure. The corrected journey confirmed
   the image and a file, read both back through retained lists, preserved the draft, and kept the
   provider request count unchanged (`...-attachment-confirm-v3/results.json`, five checks).
   The final optimized binary passed the same five checks:
   `/home/doll/.cache/peritus-tui-conveniences-20261019-attachment-optimized/results.json`.

Source review additionally corrected stale query selection, shifted-letter binding normalization,
code blocks crossing unrelated assistant answers, and code style markers living on spans rather
than lines. Query generations, exact queue revisions, separate assistant groups, and the actual
Markdown parser provide those boundaries. The color replay also needed to remove the automation
shell's inherited NO_COLOR setting; respecting NO_COLOR is not a product defect. A pyte wide-cell
exception was fixed in the driver rather than classified as an application bug.

## Replay and limits

See [the replay guide](reproducers/tui-conveniences/README.md). `--local-only --provider` exercises
the local controls with a configured real provider but does not start a provider request.
`--resume-response <owned fixture>` reopens a retained real response for the remaining checks.

Observed platform: native Linux x86-64. macOS AppKit and Windows PowerShell clipboard paths are
implemented but were not run on native systems in this campaign. Transcript search covers public
activity actually captured by the client; it cannot recover older uncaptured or disconnected gaps.
No exhaustive coverage claim is made.

## Optimized artifact and installation

The optimized CLI/TUI build used `--release --locked --offline --jobs 1` under a transient
user scope with MemoryHigh=2G and MemoryMax=3G. Optimized terminal replays checked the actual
release binaries. Final installation hashes, per-user backups, and observed daemon identities
are recorded in `/home/doll/.cache/peritus-tui-conveniences-install/installation.json`.
Only client/TUI binaries are replaced; daemon executable and running work are preserved.
Disposable daemon/helper shutdown records are in that directory's `cleanup.json`.

Installation completed for both `doll` and `builder`, with matching source/installed hashes,
correct ownership, successful per-user version/help startup, and per-user backups. No live
daemon was present at installation; no daemon binary was replaced or started. Final disposable
attachment-test daemon and clipboard/display helpers were shut down after the replay.
