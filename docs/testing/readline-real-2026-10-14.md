# Readline TUI editing real testing — issue 126

[Issue #126](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues/126) reports that Ghostty/cmux translate Cmd-Left/Right into Ctrl-A/E before the keyboard protocol is consulted. The old composer dropped those bytes. The requested readline baseline now maps character, word, row, line and buffer motions and deletions in the shared editor, while preserving modified-arrow aliases and kitty negotiation. Home/End are line-relative; Ctrl-Home/End retain whole-buffer jumps. Ctrl-K at a line ending removes its newline. Existing review-feedback Ctrl-B/F actions retain their specialized meaning. Kill-buffer/yank support was explicitly optional and is not added.

Confirmed defects and replay:

| Defect | Original observation | Final observation |
| --- | --- | --- |
| Ghostty line-motion bytes | Ctrl-A/E did not move the cursor; a marker was inserted at the old position. | Markers appear at the current line start/end. |
| Missing readline motions/deletions | Character/row motions, word/line deletion and their requested chords were dropped or inserted unwanted text. | Literal Unicode and multiline editing outcomes match the independently specified contract. |
| Home/End used whole buffer | Multiline input jumped to the first or final buffer position. | Home/End move within the current logical line; Ctrl-Home/End still reach the buffer ends. |
| Shared-caller dispatch hazard | Modal insertion and bounded-field admission treated Alt-D as a printable character before shared dispatch. | Alt-D reaches word deletion in modal, file and image fields; selected text is deleted through the selection route. |

The actual old CLI binary failed 59 of the 75 same-current-contract journeys (23 direct chord scenarios and all 36 generated variants). The changed client passed 75/75 in legacy negotiation and 75/75 in enhanced negotiation. Another 42/42 actual shared-modal/file/image-field scenarios passed. The final delivery binary also replayed the enhanced 75/75 after prompt changes. These counts distinguish executed journeys rather than whole-application coverage.

The structured generator uses seed `12620261014`, distinct Unicode words, varied whitespace, multiline positions, word deletion and line movement. Direct cases also cover empty input, edge positions, selections, shifted modified keys, release events, ordinary letters, Ctrl-H/D, Ctrl-U/K/W, Ctrl-Alt-H, Super-Backspace and preserved arrow aliases. The client exits normally and restores terminal settings. Each run used a real fixture daemon, the real compiled CLI and terminal bytes delivered through a PTY; no application model or daemon response was substituted.

Builds used `cargo build --locked --jobs 2 -p peritus-cli -p peritus-daemon` (with `peritus-tui` also selected initially) in user systemd scopes with MemoryHigh=2G, MemoryMax=3G and MemorySwapMax=0. No unit tests, Clippy, format checks, qualification or formal suites were used. Debug binaries and disposable XDG roots remained separate from the installed live daemon.

Use [the replay instructions](reproducers/readline/README.md) and [saved concrete regressions](reproducers/readline/regressions.json). Related shared-caller and earlier composer replayers now use explicit whole-buffer selection to clear drafts and honor the new Home/End contract. Raw screens, ANSI streams, JSON results and baseline/candidate binaries are retained under `/home/doll/Peritus-Prompt-Skills-2026-10-14/` outside Git.

Two driver issues were corrected separately from product findings: startup waited for actual Ready instead of assuming a three-second delay, and the terminal decoder handles pyte's orphaned wide-glyph continuation cells during repaint. Raw application escape streams remain retained. Native macOS/cmux and physical terminal shortcut routing were not available on this Linux host; the replay exercises the documented Ghostty bytes and Peritus's actual parsing/editor behavior.
