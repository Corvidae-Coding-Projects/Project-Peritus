# Real composer and conversation-mode replay

## Folder guide

Real-interface replay resources for composer modes; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [REPORT-2026-10-13.md](REPORT-2026-10-13.md) | Document: Composer mode capability replay — GitHub #122. |
| [SHARED-CALLERS-REPORT-2026-10-13.md](SHARED-CALLERS-REPORT-2026-10-13.md) | Document: Shared editor routing — actual UI replay. |
| [keyboard-regressions.json](keyboard-regressions.json) | Structured keyboard regressions data with fields `interface`, `baseline_sha256`, `fixed_sha256`, `cases`. |
| [replay-webui.mjs](replay-webui.mjs) | Browser client module for replay webui. |
| [shared_callers_replay.py](shared_callers_replay.py) | Replay shared-editor routing through real Peritus/daemon/PTY interfaces. |
| [tui_replay.py](tui_replay.py) | Operate actual Peritus via PTY, decode its screen, and replay human editing. |

These drivers operate the actual program. They contain no unit tests or substituted application implementations.

## Short text paste and output selection

`paste-replay.py --binary /absolute/path/to/peritus --fixture /absolute/private/fixture`
replays the reported text `Shared https://doc.qt.io for reading.` through the real terminal
input parser, launcher, TUI and daemon. Prepare a fresh fixture with a committed Git repository
in `project/` and `owned-fixture.json` containing `{"owned_disposable_fixture":true}`. It selects
offline provider `0`, so none of these checks invokes a model or submits the draft.

The same text must remain editable after ordinary paste, paste during F2 output selection,
and fragmented bracketed-paste delivery. The last check enters selection mode and presses
Ctrl-Q: it must exit promptly. `--expect-old-quit` records the old swallowed quit key and then
uses Esc to leave selection before exiting. Use a fresh fixture for a different binary cohort;
launcher configuration generations are immutable. Reuse installed automation dependencies with
`PERITUS_AUTOMATION_PYTHON` and `PLAYWRIGHT_BROWSERS_PATH` when appropriate.

The keyboard-paste replay passed before and after the earlier Ctrl-Q repair. The subsequently
reported mouse action exposed a separate defect: a right-click in the composer entered frozen
terminal-selection mode, swallowing subsequent typing until Esc. `mouse-paste.py --expect-old`
reproduces that sequence through the real terminal mouse protocol.

## Mouse clipboard replay

Run `mouse-paste.py --binary /absolute/path/to/peritus --output /fresh/private/fixture --backend x11`
with the replay Python environment (`pyte` required), Xvfb and xclip installed. The driver creates
and owns its Git workspace, daemon, X display and clipboard owners. It never changes the desktop
clipboard. Installed automation can be reused with the same environment variables described above.

For Wayland, use an owned virtual compositor and pass `--backend wayland --runtime-dir DIR
--wayland-display SOCKET`; the private runtime directory must contain `.peritus-clipboard-replay`
with the text `owned`. Qualification used KWin's virtual backend on a separate D-Bus session,
with private configuration/runtime directories, no lock screen and no global shortcuts. The actual
wl-copy and wl-paste processes communicate with that compositor. The driver leaves compositor
lifecycle to its owner and cleans up its own clipboard processes and exact daemon endpoint.

The mouse replay checks right-click menu responsiveness, the reported Qt link, caret insertion,
Unicode Copy and replacement, distinct clipboard/primary values, typing and caret motion during
a delayed read, the five-second timeout, terminal paste, inert pasted commands, dismissal and
resize, nontext rejection and the existing image preview. Delayed reads stop and resume an owned
real clipboard process; no helper or application response is substituted. It also asserts that
no model trace or coding run was created and the workspace source is unchanged.

The fix keeps clipboard ownership in the client and uses its existing bounded local-operation
runner. Temporarily releasing terminal mouse capture cannot deliver the original right-click
back to the terminal, so a live Copy/Paste menu replaces that implicit mode switch. F2 remains the
explicit native-output-selection action. No persisted or wire format changes are required.

Both X11 and isolated Wayland campaigns passed on the fixed program. Windows target compilation
also passed. macOS cross-compilation was blocked in aws-lc by the unavailable Apple C toolchain;
macOS runtime clipboard behavior was not exercised on this Linux host.

## Build

From the repository root:

```sh
cargo build --offline --release -p peritus-tui -p peritus-cli -j 2
(cd webui && npm run build)
```

## Terminal

Use a disposable fixture with `config/`, `state/`, `cache/`, and `repo/` directories. Initialize `repo/` with `git init`, write `owned-fixture.json` containing `{"owned_disposable_fixture":true}`, and launch `target/release/peritus open FIXTURE/repo` with `XDG_CONFIG_HOME=FIXTURE/config`, `XDG_STATE_HOME=FIXTURE/state`, and `XDG_CACHE_HOME=FIXTURE/cache`. Select restricted workspace browsing and provider `0` (offline) for this UI replay. Exit that client with Ctrl-Q. The launcher creates a real disposable daemon; this avoids inference and live-user state.

Install `pyte` in an isolated Python environment. Run:

```sh
python docs/testing/reproducers/composer-modes/tui_replay.py --binary /path/to/target/release/peritus \
  --fixture /path/to/disposable-fixture --output /tmp/composer-enhanced --enhanced
python docs/testing/reproducers/composer-modes/tui_replay.py --binary /path/to/target/release/peritus \
  --fixture /path/to/disposable-fixture --output /tmp/composer-legacy
```

`--enhanced` implements the terminal capability-query replies documented by the kitty protocol. The PTY carries real key escape sequences into Crossterm, and `pyte` decodes real Ratatui output. This establishes the program's parser, editing, presentation and ordinary-exit behavior, not macOS/OS shortcut routing or cmux support. The legacy replay reports primary device attributes without keyboard enhancement support. Neither variant substitutes a daemon or an application model.

The concrete old-version failures and final results are in `keyboard-regressions.json`. The driver replays those editing journeys and generates fresh structured word-boundary inputs from seed 121122. Expectations are literal requested editing outcomes; the checker does not copy the production word-navigation algorithm. Marker insertion makes cursor positions and otherwise invisible trailing spaces observable.

For ordinary folders, first open another disposable nongit directory through the same launcher and explicitly select its trust level. Use `--workspace FIXTURE/FOLDER --mode-only --folder read-only` or `--folder writable` to replay context-aware hints without rerunning editing exploration. A folder must be inside the disposable fixture. These checks retain `/build` rejection for ordinary folders, and verify `/chat` guidance for trusted in-place work.

Save outputs outside the source checkout. The driver closes its own client; shut down only the fixture daemon afterward through its recorded endpoint using `peritus --endpoint ENDPOINT shutdown --wait`.

## WebUI

Launch the real `peritus-web` gateway with explicit disposable `--root`, `--state`, `--config`, `--product-state`, `--daemon-config`, and `--endpoint` paths and the current `webui/dist` assets. Mode descriptions work before daemon/provider registration. Then run:

```sh
PERITUS_REPLAY_URL=http://127.0.0.1:4221 \
PERITUS_REPLAY_OUTPUT=/tmp/composer-webui \
node docs/testing/reproducers/composer-modes/replay-webui.mjs
```

Set `PERITUS_PLAYWRIGHT` to an installed Playwright module and `PERITUS_CHROME` to the real Chrome executable when needed. The replay performs mode, directory, draft, reload and accessibility interactions on the actual UI, uses one isolated browser, and submits no AI tasks. The WebUI report records its bounds and expected offline-fixture diagnostics separately.

## Shared modal and bounded field callers

See [the shared-caller report](SHARED-CALLERS-REPORT-2026-10-13.md) for actual old/fixed modal and file/image capacity failures. Initialize and trust a committed disposable Git workspace through the normal launcher before testing Workbench fields; a restricted Git profile has no registered control-domain workspace. Then use `shared_callers_replay.py --workspace FIXTURE/managed`. All edits are local drafts, with no process attachment submission, inference, preview, import or confirmation.
