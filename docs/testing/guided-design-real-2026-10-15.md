# Guided Design mode: real-interface campaign

## Contract and build

The user requested an understandable interactive mode that creates a design document Peritus actually uses, and selected a conversational interview with suggested answers and an editable draft. During testing the user also reported that the browser conversation was too small. Expected behavior was established before operation in [.design/interactive-design-mode.md](../../.design/interactive-design-mode.md).

Built the real CLI, daemon, and gateway with `cargo build --locked -p peritus-cli -p peritus-daemon -p peritus-web -j 2`; built the actual browser bundle with `npm run build` in `webui`. Used Linux, real PTYs/terminal rendering, Chrome through its automation extension, and actual ChatGPT-account inference. No provider replies, tools, application screens, or effects were mocked. The campaign did not invoke unit suites, formal verification, standalone qualification/verification gates, or Clippy suites. The explicit Build journey exercises Peritus's existing runtime check/reviewer/fixer phases as product behavior. The disposable CLI implementation is evaluated by directly executing its real interface.

All test state is under `/home/doll/Peritus-Design-2026-10-15`, with isolated XDG config/state/cache. HOME and CODEX_HOME were unchanged. The user's installed binaries and personal daemon were not replaced or restarted.

## Observations

| Scenario | Expected outcome | Observed result |
| --- | --- | --- |
| Bare `/design` in the real terminal | Explain the interview and ask one useful question with examples | PASS: a focused opening question; no workspace effects. |
| Existing greeting CLI design | Inspect the real source, save explicit requirements and real-use acceptance examples | PASS: Markdown v1 retained exact ordinary/uppercase outputs, Unicode, single-file/no-dependency scope, and a useful help question. |
| Correct an earlier decision | Replace `--shout` with the chosen replacement flag throughout the current requirements and commands | PASS in two seeded terminal journeys: `--uppercase` and `--caps`; the retired flag remains only as an explicit exclusion. |
| Browser interview and revision | Explain choices plainly, save an editable draft, and preserve revised decisions | PASS: camping checklist became bicycle touring; device-only storage, saved checkmarks, one reusable list, and sharing exclusion persisted. |
| Show the draft | Display the complete saved document | PASS through `/design show` and the browser **Show draft** button. All saved nonblank lines were present, unchanged and in order. |
| Close client, stop/start disposable daemon, reopen exact run | Preserve saved bytes and continue the interview | PASS: exact Markdown bytes survived restarts, including both independent drafts in the manual fixture. |
| Approve the design | Save ready state without editing the project | PASS: version 3, ready stage, and saved path; independent read-only replay compared every project file/path. |
| Read-only project | Inspect source and save private conversation metadata without granting workspace writes | PASS: actual config had `writable=false`; only the original project files remained, with unchanged hashes. |
| Stale update | Reject an actual old-version write and preserve current document | PASS: a real `design_draft` call with expected version 0 returned `draft changed; read the current draft before updating it`; no retry, exact bytes and ready/version 3 unchanged. |
| No saved design | Ask to start a design and avoid implementation | PASS through the browser action and latest terminal build. A follow-up host observation removed unnecessary archive searching: latest replay needed one provider request and zero tool calls. |
| Ordinary Chat after no-draft case | Answer a separate ordinary question without requiring Design | PASS: actual terminal replied `16` to the separate 7 + 9 request. |
| Explicit `/design build` | Feed corrected saved Markdown to the actual implementation designer and produce the requested CLI behavior | PASS: automatic design preserved the corrected flag and literal Unicode examples; one Python source file changed, the pipeline completed, and independent direct CLI readback passed. |
| Browser reading space | Make responses readable while retaining access to the full workspace | PASS: at the actual 1707 × 795 viewport, compact workspace gave about 264 px of transcript height and optional Focus conversation about 435 px. The previous screenshot showed roughly 64 px. Reload retained focus; Show workspace panels restored explorer/project navigation/controls. `/files` also exited focus. |

Browser operation used the real gateway at `http://127.0.0.1:4205/`. Chrome initially blocked automation from opening localhost; the user opened the page, after which its actual controls and saved state were operated and read back. Draft creation, revision, full document display, independent sessions, no-draft action, reload, Focus conversation, and restoring panels were exercised. The footer now reads the package version instead of a stale hard-coded version.

## Defects and friction corrected

- **Cramped browser conversation:** large desktop decoration, session chrome, and composer consumed almost the entire window height. Compact styling is now height-aware; response typography is larger, and the optional persistent focus control gives the transcript most of the workspace. The user saw the focus state during testing and requested context; the full workspace was restored. Files/Git navigation exits focus so panels remain discoverable.
- **Needless no-draft archive searches:** the model lacked an explicit host fact that no collaborative draft existed and searched context archives twice. Chat now receives that exact observation, explicitly preserving ordinary Chat requests. A fresh real replay returned the appropriate single question with one inference and no tools.

## Driver corrections and existing boundaries

The first retained Show oracle required byte-identical Markdown as a substring of assistant prose. The actual model added blank lines after headings, preserving every content line. The oracle now ignores presentation-only empty lines for this prose/inline-code fixture, keeping content and order exact. Raw artifacts preserve the original false failure.

Reopening a terminal run can initially display history before its authenticated write destination is bound. The real UI retains the draft and says `Send again after it opens.` The driver originally treated the visible history as sufficient and waited for an input it had not submitted. The corrected driver observes the retained draft/binding notice, confirms that durable input count has not advanced, then follows the UI's explicit instruction with one additional Enter. No approved design, user input, or workspace effect was lost. These were driver issues, distinct from application failures.

Existing workspace execution admission is exclusive. One browser message submitted while the other terminal conversation owned the same workspace was durably queued with an explicit conflict, then incorporated when continued after the workspace became idle. This campaign does not claim concurrent execution of two conversations in the same workspace.

## Replay and evidence

Portable PTY replay and browser instructions: [reproducers/guided-design](reproducers/guided-design/README.md).

```sh
uv run --with pyte==0.8.2 --no-project python docs/testing/reproducers/guided-design/replay.py \
  --binary /absolute/path/to/current/peritus \
  --output /absolute/new/disposable-directory --seed 20261015 --build
```

The fixed seed selects a replacement flag; a second independent seed selected a different one. The implementation consumer uses six literal stdout expectations covering plain/uppercase, Unicode accents/CJK/emoji, and a name containing a space, plus real help discovery. This is bounded feature exploration, not whole-application or cross-platform coverage.

Raw evidence locations:

- `fixture/`: manual terminal/browser drafts, durable records, and actual browser session state.
- `before-restart.json`, `ready-design.md`, `*.screen.txt`, `*.ansi`: exact saved versions and terminal observations.
- `final-replay/`: final binary interview, corrections, readback, restart, approval, implementation, and CLI effects.
- `independent-readonly/continued-result.json`, `approved-design.md`, `05-stale-activities.json`: independent read-only and actual stale-update evidence. Its owned daemon/client were stopped.
- `latest-interface/`: latest no-draft and ordinary Chat checks.
- `web-build-final.log`: final actual Svelte/Vite bundle build.

Only the Chat no-draft observation changed after the main immutable replay binary was prepared; the latest binary independently exercised that branch. The Design tool, persistence, read-only policy, and implementation-document loading were unchanged by that adjustment. Browser assets were rebuilt after the focus/navigation changes and operated through the existing gateway. The preview was then reconnected to the latest daemon with the saved documents intact.

## Final handoff outcome

The actual pipeline completed in place with one changed file (`greet.py`). Its generated implementation design used `--uppercase` and retained `HELLO, CAFÉ 雪!`; the retired `--shout` remained only as an exclusion. The writer executed 13 real CLI operations, and Peritus completed its existing runtime checks and independent review. The complete project still contained only `greet.py` and its original `AGENTS.md`.

The separate consumer then executed six cases against the resulting actual Python program: ordinary and uppercase Ada, ordinary and uppercase `café 雪`, uppercase `Zoë 🚲`, and ordinary `two words`. All stdout bytes and exit statuses matched the literal expected results, including one trailing newline. Actual `--help` exposed the corrected flag and example. The consumer confirmed the obsolete flag was absent from the implemented source and that the downstream design contained the corrected choice. `final-replay/result.json` records `passed: true`.

The main immutable binary's cumulative run had 25 provider requests, 20 tool calls, zero retries/failovers, and roughly 76 MiB peak observed daemon RSS. These counts include the interview, editing, full readback, approval, and implementation pipeline. The independent read-only replay also records `passed: true`, including the stale-write rejection. Latest no-draft and ordinary Chat each used one inference with zero tools.

Owned replay clients and daemons were stopped. One intentional preview remains at `http://127.0.0.1:4205/`, backed by the latest disposable daemon; `preview-process.json` identifies its ownership/configuration. It uses the synthetic fixture project, not the user's working project. The personal daemon and installed program were left running unchanged. This campaign establishes the exercised Linux/ChatGPT-account feature behavior; native macOS/Windows and other providers were not exercised.
