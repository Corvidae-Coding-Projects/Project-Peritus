# Composer navigation and mode explanations — GitHub #121 / #122

Implemented on `develop`, based on commit `038d47dd78ae9b636861dfc8e1d4d13299bab33b`, with the changes in the working tree. This is a bounded real-interface campaign, not a claim of application-wide or native macOS coverage. No unit tests or repository/formal verification gates were run or added.

## Observable requirements

- Option/Alt-arrow and legacy Alt-b/f move by word while Ctrl-arrow retains its behavior.
- Command/Super-arrow moves to the current logical line's boundaries. Home/End retain whole-message behavior.
- Shift extends selection with these motions; replacing the selection preserves the surrounding draft. Existing selection is retained as a feature.
- Option/Alt/Ctrl-Backspace deletes the previous word; Unicode and empty/boundary inputs remain editable.
- Negotiate optional keyboard enhancements before the input worker starts, retain legacy fallback, and restore terminal modes on exit. Pop enhancements before leaving the alternate screen or handing it to a foreground child; restore them when resuming.
- Explain all four conversation modes before submission, persistently in the TUI composer and WebUI, with fuller help/catalog descriptions and accessible WebUI descriptions. Mode selection remains explicit; restrictions and permissions remain authoritative in the daemon.
- Respect ordinary-folder behavior: trusted in-place folders direct users to `/chat`, while restricted folders stay visibly read-only. `/build` rejection is retained for ordinary folders.

## Implementation

The shared text editor classifies cursor motions before character insertion. The modal insertion route and file/image capacity checks use that same classification, so navigation is not mistaken for typing before the shared handler runs. Selection uses the same classification, so Alt-b/f cannot replace selected text as a typed letter. Super-modified character shortcuts do not insert unwanted characters. Existing Unicode word helpers are reused; the line helpers use newline boundaries and preserve byte-safe cursor positions.

Terminal ownership now detects Crossterm's optional keyboard enhancement support and pushes only `DISAMBIGUATE_ESCAPE_CODES`. Ownership includes the attempted push for failure cleanup, and cleanup pops it on the alternate screen before restoring mouse, paste, cursor and raw-mode settings. Detection occurs before the blocking input worker; resume reuses the detected capability rather than racing that worker with another query.

TUI mode descriptions are shared by help, slash-command metadata and mode-selection notices. The composer title prioritizes the mode and read-only boundary even at 40 columns. It gives `/chat` guidance in trusted ordinary folders, `/build` guidance in managed-workspace contexts, and identifies restricted folders. WebUI buttons, catalog details and persistent composer explanations share one description table; buttons and input expose those explanations through accessibility relationships. The persistent paragraph remains visible in the mobile layout.

## Build and actual interfaces

```sh
cargo build --offline --release -p peritus-tui -p peritus-cli -j 2
(cd webui && npm run build)
```

The final CLI SHA-256 is `603cf2ba6369cfc2e93c88e7affbd84817904ee563d7ea8f59981604ce921edf`. The first complete composer campaign used intermediate SHA `0f217720446983589b2d404fd96acc375c73e9b49e8f28bf6d6d7ba919f180e2`, before final shared-caller and teardown refinements. The actual installed pre-change CLI used for the comparison is `865293f480b3f12103e20f3e3927d507995bdcac01aea0a4e535be7c83ec445e`. Both were launched through `peritus open` with exclusively disposable XDG state/config/cache and real launcher/daemon initialization under `/tmp/peritus-ui-issues-20261013/`. Provider `0` prevented inference. The replay inspected the actual rendered terminal output and OS terminal settings; it did not substitute a TUI model or daemon.

The PTY consumer implements terminal capability-query replies and uses `pyte` to decode real Ratatui screen updates. It is a terminal protocol replay, not proof that macOS or cmux delivers a physical shortcut. The legacy negotiation run also injects extended encoded keys to check parser robustness; actual legacy terminals may not deliver Command/Super keys.

The WebUI used the real gateway at `http://127.0.0.1:4221`, current built frontend assets, and one isolated Chrome instance with Playwright. Its root, configuration, saved browser workspace and daemon endpoint were isolated. It intentionally had no registered provider/daemon. It made no API response substitutions and submitted no AI task. Detailed browser results and provenance are in [the WebUI report](reproducers/composer-modes/REPORT-2026-10-13.md).

## Results

| Real journey | Result |
| --- | --- |
| Pre-change CLI comparison | 48 of 61 editing/lifecycle scenarios reproduced missing new behavior; unchanged Ctrl-arrow, Home/End, empty-buffer safety and orderly cleanup continued to work |
| Final enhanced terminal replay | 78/78 scenarios passed on the final CLI, including 36 generated word-navigation inputs, legacy and enhanced Alt-b/f encodings, Shift replacement, Unicode/newlines, release/repeat events, deletion, mode hints, help, resize and encoded Ctrl-Q exit |
| Final legacy-negotiation replay | 78/78 scenarios passed on the final CLI, with the same editing and presentation journeys and preserved legacy negotiation |
| Restricted ordinary-folder replay | 14/14 scenarios passed; modes stayed visibly read-only, ordinary-folder `/build` remained rejected, help and hints survived resizing |
| Trusted ordinary-folder replay | 14/14 scenarios passed; Plan/Review recommended `/chat` instead of the rejected `/build` command |
| Real WebUI replay | All direct scenarios plus 18 generated input cases / 72 mode switches passed at 1440×1000, 768×1024 and 360×800; titles, Chromium accessibility descriptions and catalog details agreed |
| Browser draft and state recovery | Unicode/multiline drafts and selected Review mode survived reload and a fresh page; opening help did not change mode |
| Supplemental shared editors | 3/3 modal fixes and 21/21 managed-workspace field/cleanup scenarios passed; ten capacity-navigation failures reproduced before correction |
| Keyboard-mode cleanup | Enhanced replay observed one push and one pop; legacy observed neither; clients exited 0 and restored their original `termios` settings |

The terminal generator uses seed `121122` and builds two distinct words with varied spacing, encoding and action order; expected insertion boundaries come from the requested word-navigation contract. The browser generator uses seed `12220261013`, six draft cases at each viewport, and explicit varied mode transitions. Empty drafts, combining marks, Arabic, emoji, long inputs and newlines were included. Counts describe executed interface scenarios, not coverage percentages.

Confirmed old-version defects are retained with concrete input, key bytes, expected text, old output and fixed output in [keyboard-regressions.json](reproducers/composer-modes/keyboard-regressions.json). For example, Option-left produced `alpha bet|a` instead of `alpha |beta`; Alt-b produced `alpha betab|`; Command-left in a two-line draft moved one character rather than to the second line's start. Each retained replay passed on the changed executable.

Two checker errors were corrected without relaxing product requirements: terminal extraction trims screen-padding spaces, so deletion cases now insert a marker to make the preserved trailing space observable; ordinary-folder `/build` intentionally retains its rejected command draft, so the driver clears it before the next distinct command instead of concatenating slash commands. The WebUI driver corrected CSS-capitalized name comparisons. These were checker failures, not product defects.

Browser logs had no page exceptions. Fifteen HTTP-400 resource notices came from the deliberately offline fixture's unavailable daemon and unregistered workspace metadata; they are recorded separately from mode-feature outcomes. No unresolved failure in the exercised changed-feature paths was observed. An initial restricted/unregistered fixture boundary blocked supplemental field setup and is recorded below; it is not an established application regression.

## Additional shared-caller exploration

An independent review found three caller filters that still bypassed the new shared navigation: modal `Char` insertion intercepted Alt-b/f and Super shortcuts, while file/image capacity prefilters rejected Alt-b/f at full length. The actual modal replay reproduced `alpha betab|` for Alt-b, `f|alpha beta` for Alt-f from Home, and an unwanted `b` for Super-b on the intermediate CLI. The changed modal replay produces `alpha |beta`, `alpha |beta`, and unchanged `alpha beta|`, respectively. These are confirmed real-interface failures and resolved real-interface replays.

The image/file prefilters were corrected at their insertion boundary without loosening size limits. Supplemental field-capacity replay uses a separately initialized, committed and trusted disposable Git workspace through the real launcher, with the provider still offline. The final real field replay passed 21/21 scenarios: ten Alt-b/f motions at capacity, ten retained insertion-limit checks, and orderly exit. Both directions were exercised in file path (4096 bytes), range (80), caption (8192), and image path (4096)/caption (8192). [The shared-caller report](reproducers/composer-modes/SHARED-CALLERS-REPORT-2026-10-13.md) preserves the concrete failures and replay commands.

The initial supplemental `/files` setup in the restricted Git fixture `/tmp/peritus-ui-issues-20261013/repo` returned `session-mismatch` and no metadata snapshot, including after one bounded `/new` plus reconnect attempt. That profile has no registered managed workspace in the daemon's control domain. It is a **fixture registration/trust boundary**, not evidence of a trusted ordinary-folder defect or a regression from this change. The supported launcher then initialized and trusted `/tmp/peritus-ui-issues-20261013/managed`, and the actual file/image controls became reachable. No provider or inference task was used to bypass setup. Earlier commentary describing that initial blocker as an ordinary-folder failure was corrected after checking the driver's exact launch path.

## Limits and replay

Native macOS/cmux physical shortcuts, OS/terminal interception, Windows console behavior, foreground-candidate suspend/resume, forced terminal-output errors and active inference mode changes were not exercised here. Suspend/resume and failure cleanup were reviewed in the actual owner and caller code; only ordinary exit was directly replayed. Screen-reader application behavior beyond Chromium's accessibility tree remains unobserved. Read-only enforcement and build-pipeline execution were unchanged and were not inferred from UI labels.

Use [the replay instructions and drivers](reproducers/composer-modes/README.md). Raw actual terminal screens, escape streams and JSON results are under `/tmp/peritus-ui-issues-20261013/tui-final-current-*` (plus supplemental `shared-callers-*` and `shared-fields-*` roots); browser screenshots, DOM/ARIA observations and bundle provenance are under `/tmp/peritus-ui-issues-20261013/composer-evidence/`. Preserve the concrete regression file and drivers; bulky exploration output stays outside Git. Only campaign-owned clients, browser and gateway/daemon resources are cleaned up. The live user daemon/client and active browser-building run are unaffected.
