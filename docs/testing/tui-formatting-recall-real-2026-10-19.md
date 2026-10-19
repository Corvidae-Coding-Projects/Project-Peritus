# TUI formatting and message recall — real-interface campaign

## Contract and independent expectations

The changed CLI/TUI/daemon must render actual retained Markdown tool results and skill text with
headings, emphasis, lists, code, links, and tables. Continuation chunks belong to one document.
Source files, command output, and edits remain literal and readable; raw document details remain
available. Rendering must not activate terminal controls or fetch linked resources.

Recall is scoped to the selected saved conversation, survives reopening, and never submits a
message by itself. Up/Down recall at composer boundaries, preserve multiline navigation, retain
edits to recalled entries, and restore the exact unsent draft. F3 filters history and Enter loads
a draft, without inference or a ledger mutation. F4 inspects instructions without losing a draft.
Ctrl-K/U/W followed by Ctrl-Y must restore the removed Unicode text. Reconnect, paging, stale
responses, and terminal resizing must preserve drafts and scope.

## Method

Build and operate the actual binaries through a PTY with pyte recording terminal cells and bytes.
Use a disposable retained real-provider run, never user work. Check visible UI independently
against original submitted messages, known document contents, and unchanged durable input/model
counts. Generate varied terminal sizes and editor actions with a fixed seed. No unit tests, mock
provider, simulated TUI, or repository verification gates.

## Results

Optimized terminal campaign passed in `/tmp/peritus-formatting-recall-release-2/`:

- Actual retained 14 KB skill formatting, joined continuation sections, literal removed/added
  code, and raw document details.
- Paused only the disposable daemon to hold an actual history request. Typing while its reply
  was delayed survived both delivery and Escape. A reconnect during a held query recovered
  recall; the original draft survived.
- Up/Down, edits to recalled entries, exact Unicode draft/caret restoration, multiline caret
  movement, Shift-selection, filtered/no-match search, Enter-to-draft, and Escape restoration.
- Ctrl-K/U/W + Ctrl-Y, including a Unicode line-join case. F4 instruction inspection and a real
  Ctrl-R reconnect retained unsent text.
- Created 37 pending inputs through actual `/queue add` controls in a separate conversation.
  Verified newest-page recall, case-insensitive search reaching the oldest page, isolation from
  the original conversation, and reopening by durable identity. No model work was started.
- Thirteen generated terminal dimensions, seed 20261019, from 36×14 to 180 columns. Search,
  selected previews, and the unsent draft remained usable; closing search restored the draft.
- The original run's interaction record and cancelled phase remained unchanged.

`/tmp/peritus-markdown-documents-release/` retained the signed-in provider's actual reads of
`DOC.md`, `source.py`, and real-testing's `SKILL.md`. Markdown headings, emphasis, links, tables,
and a 190-line fenced block were displayed through the actual CLI/TUI. Code line 150 remained
styled as code across the protocol chunk boundary, literal `**stars**` remained intact, a genuine
`sha256:` document line survived, an encoded terminal control stayed inert, and Python source
remained literal. `/details` exposed the original numbered file-read output.

Independent review identified new-feature edge cases before deployment: delayed-reply Escape
could restore an obsolete draft; interrupted reads could remain loading; pasted filters did not
page; and global hash-line filtering could remove genuine document text. The implementation now
fences the complete draft, abandons stale recall state, clears interrupted reads, dispatches paste
search effects, and preserves hash-like document lines. The actual delayed/reconnect/pasted-page
journeys above passed after these corrections.

The controller initially misidentified truecolor black (`000000`) as an absent code style and
assumed a reopened conversation skipped its existing metadata panel. Those were checker defects;
expectations were corrected against captured terminal cells and the documented UI. Early scrolling
also flooded the debug client with queued navigation; final replay uses the real End control and
paced PageUp keys. No application behavior was weakened to obtain a pass.

## Final startup and saved-context corrections

A fresh-client smoke (without earlier scrolling) exposed an actual startup race: a retained run
was visible before its durable conversation binding arrived. Immediate Up/F3 therefore said to
open a saved conversation and discarded the requested recall. The client now defers that read
against the exact RunId and complete draft, obtains/deduplicates authenticated binding, and resumes
only that scoped read. Escape, new typing, rejected binding, reconnect and conversation changes
revoke it. F4 uses the same deferral and never creates a conversation merely to inspect instructions.
The debug actual-client smoke passed afterward; final optimized replay is recorded below.

Saved Markdown context is classified from explicit source metadata. Only native file-read line
prefixes are removed for its human Markdown projection; skill bytes, genuine numeric text and
hash-like content remain. An actual provider retrieved the original DOC.md tool observation by
its exact durable handle; headings and the continued code fence rendered correctly. A query-based
search initially returned binary archive excerpts, which are not Markdown documents; the checker
was corrected to request the exact native file observation. Literal excerpts remain literal. The first exact-handle request asked for 65536 bytes while this
fixture's context-read tool advertised 256–16384. It returned that explicit rejection; the provider
retried with a supported page size and successfully recovered the full native observation. This
was an invalid requested page size, not a document-rendering failure.

Search owns its modal surface: mouse dragging there cannot select hidden transcript text.

## Final binary and installation

Final optimized CLI SHA-256:
`5444ba6d07ac9df253b20bd2b98ad0d2d40580cedf61d187d7e257dff4e95bd4`.

- `/tmp/peritus-readable-tui-final-smoke/result.json`: PASS on the exact final build for
  immediate recall, filtered search, modal mouse ownership, queue inspection, draft restoration,
  and real reconnect, with unchanged original interaction.
- `/tmp/peritus-markdown-documents-release/result.json`: PASS on the same final build, including
  the provider's retained exact-handle saved Markdown observation. No new inference was needed
  for this final presentation replay.
- `/tmp/peritus-startup-history-independent-20261019-v2/result.json`: independent PASS in three
  fresh clients, pressing Up/F3/F4 about 26 ms after the first visible retained run. Exact
  original instruction and draft restoration passed. Aggregate, event, command, and original
  interaction state remained unchanged; no input or model work was submitted.
- `/tmp/peritus-readable-tui-installed-smoke/result.json`: PASS through the actual installed
  doll CLI, not the build-output path.
- `/tmp/peritus-builder-readable-tui-2/result.json`: PASS through builder's installed client,
  attaching read-only to its actual running job. Recall/search opened and the same daemon PID
  `2554610` remained running. No message, retry, stop, or restart was submitted.

Installation manifest: `/tmp/peritus-readable-tui-installation.json`. Both accounts' client and
standalone TUI match the optimized build hashes and have correct user ownership/mode 0755. Doll
also has the updated daemon. Builder's active daemon executable was preserved so the launcher
can continue attaching to that job; its new daemon is staged at
`/home/builder/.local/state/peritus/pending-updates/readable-tui-20261019T164825Z/peritusd` for a
planned graceful restart. Current client formatting/recall features work with that preserved
host; the new saved-context Markdown host projection activates with the staged daemon.

Old binaries are backed up privately. Doll's original rollback copy is
`/home/doll/.local/state/peritus/binary-backups/readable-tui-20261019T164812Z`; builder's is the
`readable-tui-20261019T164825Z` directory in its own binary-backups folder.

Two installation-checker mistakes were corrected: the standalone TUI has no `--version` option,
so its hash and actual startup were checked; a sudo test initially inherited doll's PATH and
selected a different Codex executable, so builder's reattachment was rerun in its normal login
shell environment. Its actual provider/executable configuration and active worker were preserved.

## Bounds

This is a Linux PTY/actual-daemon/signed-in-provider campaign for the changed presentation and
composer behavior. No unit tests, formal verification, or repository gates were run or added.
It does not claim all Codex features, exhaustive TUI coverage, or other operating systems.


