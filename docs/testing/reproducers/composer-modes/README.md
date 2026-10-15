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
