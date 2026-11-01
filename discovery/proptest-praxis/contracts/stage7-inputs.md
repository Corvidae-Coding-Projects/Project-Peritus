# Stage 7 input lane: bounded TUI paste

## Source and entry path

The discovery consumer source-mounts the current `peritus-tui` implementation in
`crates/app/peritus-tui/src/model/editor/input.rs`. Production `AppModel` routes
`crossterm::event::Event::Paste` to `paste_editor_event` in
`crates/app/peritus-tui/src/model/interaction.rs`; the mounted method is called
with a minimal discovery shell containing only the editor fields it touches.
The fixture does not drive a native PTY or the full action reducer.

## Independent contract

The model filters control scalars except tab and newline, bounds the captured
text to 8193 scalars, and compares the final UTF-8 byte length with the 8192
byte workbench input limit. Accepted paste inserts at a valid scalar boundary
and advances the byte cursor by the inserted byte length. Rejected paste keeps
the draft, cursor, and command provenance; it emits the retained-draft warning.
The command provenance flag is set only after an accepted paste on a
slash-prefixed draft. This matters because a later Enter dispatches pasted
slash text as a message rather than executing it as a command.

Fixed cases include the exact byte cap, `Shared https://doc.qt.io for reading.`,
Unicode and filtered controls, and rejected oversize with plain and slash
drafts. Generated cases vary scalar cursor position, Unicode/control contents,
byte-limit boundaries, and follow-up keyboard/paste operation sequences. The
fixture calls the production insertion method for keyboard characters and the
production paste method for bracketed paste. Native event routing remains a gap.

## Current result and replay

The first three fixed cases pass. Fixed slash-draft oversize case fails: the
buffer and cursor remain intact, but `pasted_command` changes from false to
true. Seed 7319 generated and shrank the same mismatch to initial `/runs`
(5 bytes) and 8188 ASCII `x` bytes. Exact replay of
`runs/stage7-tui-paste-search/minimized-failure.json` fails identically. The
pre-fix production source was preserved in the reports before a focused
change. `paste_editor` now returns whether insertion was accepted, and
`paste_editor_event` updates command provenance only on acceptance.

After that change, five fixed cases pass, seed 7319 passes 256 generated cases
(534 paste operations, 570 typed characters, 52 oversize rejections), and the
exact minimized pre-fix witness replays successfully.

Observed reports: `runs/stage7-tui-paste-examples/report.json`,
`runs/stage7-tui-paste-search/report.json`, and
`runs/stage7-tui-paste-replay/report.json`, and matching post-fix reports under
`runs/stage7-tui-paste-postfix-{examples,search,replay}/report.json`. They were built from
`d106c9b13318fbcdd3d816ed63ae48532806fa7e` with the source and harness
fingerprints embedded in each report. The source-mounted fixture is evidence
for the method contract; a full TUI event test is still needed to confirm the
end-user path.

## Uncovered work in this lane

- TUI keyboard shortcut/motion handling, mouse selection/paste, and native PTY
  event routing are not tested by this fixture. Source-mounted generated
  sequences cover typed character and paste insertion methods only.
- App-client request/response matching has only source inspection; no fake
  daemon socket has been exercised here.

## Real CLI malformed-grammar probe

`stage7_cli_probe.py` invokes the built `peritus --json web` command for eight
invalid operand forms under each of two environments. It independently expects
usage exit code 2, a JSON `usage` category, and no web data directory created.
One environment has a disposable HOME/XDG data root; the other has neither
variable. No tested command reaches a network operation.

Before the CLI fix, all 16 checks failed: with a disposable data root, the
commands reported usage but created the web store directory; without HOME/XDG,
store initialization failed first and returned internal exit 70. The source
constructed `Store::for_user()` before reading required operands and numeric
arguments. The focused change builds and validates the command input before
opening the store. After rebuilding the actual CLI binary, all 16 checks pass.
The pre/post reports embed both the exact CLI binary SHA256 and `web.rs` SHA256:
`runs/stage7-cli-web-prefix/report.json` and
`runs/stage7-cli-web-postfix/report.json`.

The probe checks malformed grammar and side effects only. Valid web searches,
source reads, and network behavior remain outside this lane; browser/web
operations have a separate Stage 7 lane.
