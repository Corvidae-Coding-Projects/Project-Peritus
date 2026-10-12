# Browser and native GUI real-testing campaign

Baseline: `develop` commit `1527bf4dd02d186770f1258f74bad89a77af1829` plus the computer-automation changes recorded in this campaign. Fedora KDE Wayland; real Chrome/Chromium, KWrite and Zenity. Native isolated workflows use Xvfb. No unit, formal, repository qualification or Clippy suites were run or added. Peritus’s own design/check/reviewer/fixer product workflow remains intact and was exercised.

## Build and origin

```sh
CARGO_BUILD_JOBS=2 cargo build --release --offline --bins --package peritus-cli --package peritus-daemon --package peritus-tui --package peritus-sandbox-linux
target/release/peritus automation setup
target/release/peritus automation status
```

Setup completed against the actual per-user runtime. The final KDE-isolation build completed in 2m11s. Final daemon SHA-256: `757546acf9d79415c306be0e7622a4935c573afea549af539aae4c9afda18ddf`. The intermediate scoped-menu model run used daemon `0221c3dadf939e50cbcbfc816265a8c3d4764d8d0b2c770b85291bef18e0e7eb`. The preceding complete native workflow used daemon `8a5415645b8c8dc11ef3fece5ca8647ce5215e1d8af3458ebc1758bd1e353927`; the browser workflow used the earlier recorded browser build.

Artifact/source identities and ordinary disposable fixtures are retained under `/home/doll/Peritus-computer-automation-2026-10-11/`. `build-origin.json` identifies the successful browser/provider build; `build-origin-final.json` identifies the preceding native build; `build-origin-menu-final.json` identifies the intermediate menu/evidence build; `build-origin-kde-final.json` identifies the final KDE-isolation build.

## Observed behavior

| Scenario | Result | Independent evidence |
| --- | --- | --- |
| Browser setup/status and system Chrome launch | PASS | Actual packaged CLI and driver, real page and decodable PNG |
| Chromium fallback | PASS | Restricted PATH excluded system Chrome; actual browser launch succeeded |
| Seven generated contact save sequences | PASS | Service consumer readback for Unicode/emoji, Arabic, empty, long, quoted/backslash, newline and seeded distinct values |
| Stale observation and coordinate bounds | PASS | Seven stale calls and three outside-image coordinates rejected; saved service data remained unchanged |
| Explicit/unexpected JavaScript dialogs | PASS | Authorized confirmation accepted; subsequent unconfigured confirmation dismissed with actual Cancelled UI |
| Workspace upload and download | PASS | Uploaded actual fixture; downloaded JSON independently parsed and compared with saved data |
| Duplicate operation identity and conflicting arguments | PASS | Same-ID new-tab replay produced identical receipt; conflicting arguments rejected |
| Existing browser attachment/detachment | PASS | Separately started Chrome remained alive; all four tabs survived detachment |
| Native multiline edit/save sequences | PASS | Three KWrite sequences independently read from disk; owned process/display cleanup |
| Native Unicode accessible controls | PASS | Zenity entry, AT-SPI value and actual program stdout agreed |
| Existing native attachment/detachment | PASS | Explicit fixture PID retained; detachment preserved the application |
| Native modal action timeout recovery | PASS | Save opened its dialog despite an AT-SPI timeout; a fresh scoped snapshot enabled the correct native save |
| Closed app recovery | PASS | Fresh observations reached closed state without unrelated images; cleanup succeeded |
| Failed isolated native startup | PASS | Invalid program rejected and its Xvfb process cleaned up |
| Native Wayland semantic input | PASS | Existing selected Zenity received Unicode through accessibility and returned it on stdout |
| Native Wayland image/pixel path | BLOCKED/UNSUPPORTED | Host active-window dimensions failed identity checks; raw Wayland pixel injection is not implemented |
| TUI → provider → real browser → independent reviewer | PASS | Completed run `1b861033aaf05e87c4b39d585476784c`, visual-only oracle and persisted service state |
| Native TUI workflow after compact ordered evidence repair | PASS | Completed run `71425c6f3f0ab2055ee09cccb9185f06`; exact 45-byte proof/report and blocker-free independent review |
| Fixed local input-token ceiling removal | PASS | Actual D0/live Codex provider accepted a 238,853 estimated-token request and returned exact independent first/last markers |
| Qt File-menu scoped discovery | PASS for discovery | Model run `155466970fcd56444b446b7c1b33afec` expanded the branch; the earlier claim of visible popup opening was corrected in CA-011 |
| Isolated Qt menus remain on their display | PASS | Corrected direct replay and completed final model run `1729c2d5872a3333afab5a1e28f03f57` show the real popup and New Window control |
| Owner disappearance and attached-app preservation | PASS | Real owner-exit replay closes owned KWrite, removes readiness and obtains a fresh live snapshot from attached Zenity |
| Live permission revocation | PASS at provider admission | TUI restricted network; run `552d676bd5e969316e852f076d3efc61` was denied before any provider request or tool; no browser started |

Exploration seed: `260211`. The generated browser budget was seven distinct save/readback sequences plus the listed lifecycle and invalid-input transitions. Native generation used three distinct replacement/save sequences; the retained regression replay separately exercises repeated-position clicks, multiline input, Unicode accessibility, attached-app preservation and closed-app recovery. No exhaustive application, multi-hour horizon, Windows-native or macOS-native coverage is claimed.

## Actual model workflows

The browser task required the model to read a random word drawn on a canvas, enter a contact using the real controls, save, and write a report. The page implementation and separate oracle were outside the selected workspace. The trace contains no `evaluate`/JavaScript/source inspection workaround. `report.json` contains `VISUAL-316E4A`, matching `visual-oracle.txt`; the independently read service state matches the saved name/note. Five registered real images reached durable requests, including the independent reviewer’s fresh snapshot. Run completed with 18 provider requests, 17 tool calls and one compaction.

The first native TUI run created the exact 45-byte two-line document and matching JSON report, and closed owned KWrite. It corrected an accidental suggested filename through native editable text. Its independent reviewer then failed from repeated hidden-menu trees copied into the immutable command brief: 39 provider requests, 16 compactions, and estimated context 202,723. This remains a recorded failure, CA-008, alongside the successful direct file readback. A second replay exposed missing ordered workspace events in review evidence (CA-009); its already-correct artifacts were preserved, and its active fixer was explicitly cancelled for repair.

Final run `71425c6f3f0ab2055ee09cccb9185f06` used another fresh workspace and the unchanged native task contract. It completed with 22 provider requests, 20 tool calls, one compaction and three genuine registered native images. Independent review found no issues and explicitly verified declaration → GUI write/save → close → workspace read → report creation. The independent file and JSON readbacks both match the predeclared oracle. Only the two expected outputs and the two original files remain.

The expanded native replay `155466970fcd56444b446b7c1b33afec` completed with the same exact 45-byte proof, valid report including menu_label, 31 provider requests, 30 tool calls, seven compactions and 13 registered images. Its scoped tree discovery passed, but later image inspection disproved the checker's assumption that Ctrl+M/Alt+F visibly opened File. CA-011 records that correction and the resulting KDE isolation repair.

Final targeted run `1729c2d5872a3333afab5a1e28f03f57` exercised the repaired isolated child environment and updated instructions. The model invoked the observed real File action, received the genuine popup image, scoped a fresh snapshot to its showing popup, and read the showing New Window control without activating it. It closed owned KWrite, wrote menu-report.json and read it back. Independent review returned no findings. The run completed with 20 provider requests, 22 tool calls, one compaction and four registered images. An independent image inspection and JSON readback agree; both original workspace inputs remain unchanged.

The token-ceiling replay is a small real consumer linked against the built production libraries and the actual account-backed provider. It declares no host tools and supplies ordinary text fixture data with independently chosen markers at both ends. The trace reports `DISPATCH estimated_input_tokens=238853`, actual provider envelopes, `RESULT FIRST-17 LAST-93`, and exit 0. The profile’s former 200,000 value remains a soft compaction target; Peritus no longer rejects requests solely because their estimated token count exceeds it. The backend still enforces its actual capacity.

## Resource control

After the user reported a host panic/resource lockup, builds and fixtures were run sequentially in user systemd scopes with four CPU cores maximum and lowered scheduling priority. Builds had MemoryHigh=6GiB, MemoryMax=8GiB, TasksMax=256; fixtures had MemoryHigh=3GiB, MemoryMax=4GiB, TasksMax=192. Available host memory was approximately 20–24GiB when these limits were selected. The final build’s observed memory peak was 3,326,132,224 bytes (about 3.10GiB). Native model tasks use an isolated Xvfb display and private app profile.

The available previous-boot journal did not establish a kernel panic or OOM cause; recorded i915 display errors also predated this task. The user’s reported lockup is retained as a host incident, without claiming a diagnosed cause or attributing it to a particular program. No deliberate stress or panic reproduction was attempted.

## Defects and replay

[Confirmed defects and repairs](computer-automation-defects-2026-10-11.md) record CA-001 through CA-011, including initially failing runs and final outcomes. Native defects have a retained actual-interface replay:

```sh
target/release/peritus automation status
python3 docs/testing/reproducers/computer-automation/replay-native-text.py
python3 docs/testing/reproducers/computer-automation/replay-owner-exit.py
python3 docs/testing/reproducers/computer-automation/replay-native-menu.py
```

All three retained native replays passed against the final KDE-isolation build. The corrected visible-menu replay additionally requires a showing control and actual popup image. For the model-level replay, open a fresh trusted artifact folder containing `peritus-workspace.toml` with `schema_version = 1` and `kind = "artifact"`, and submit the retained `native-replay-task.txt` from the campaign directory through Peritus’s TUI. Independently compare `native-proof.txt` with `native-expected.txt`, parse the report, inspect the actual GUI trace and require a completed independent review. Ordinary fixture clients and earlier campaign outputs are retained outside the repository; their old live session IDs are historical evidence rather than runnable sessions.

## Cleanup and boundaries

The final public daemon shutdown completed cleanly with zero remaining processes. The TUI exited and its bounded scope became inactive. No OOM, memory-limit hit or high-water event occurred in that final scope; peak memory was 284,647,424 bytes (about 271MiB) in the final menu run. The preceding full file/menu run peaked at 454,410,240 bytes (about 433MiB), and the earlier full file-only run peaked at 180,236,288 bytes (about 172MiB). Test network permission was restored through the TUI. All replay controllers, apps and private displays were stopped. Original user browsers/apps are preserved. Local replacement is performed separately after the authorized commit and push.

Network revocation blocks account-provider admission itself, so that negative test did not directly reach the lower browser_control permission branch. Per-tool denial and cleanup invoked through a model after revocation remain unexercised; they are not counted as separate passes. Browser attachment needs an existing explicitly selected loopback CDP endpoint. Controller/daemon restart requires reopening or reattaching live GUI sessions; no serialized browser-object recovery is claimed.
