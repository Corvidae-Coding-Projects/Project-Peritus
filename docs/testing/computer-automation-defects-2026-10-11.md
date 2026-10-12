# Computer automation: confirmed real defects

Campaign baseline: develop `1527bf4dd02d186770f1258f74bad89a77af1829` plus the computer-automation working diff.
Actual Chrome, KWrite and Zenity were operated on Fedora KDE Wayland, with private Xvfb displays
for isolated native workflows. No unit or formal suites were used.

## CA-001 — Multiline native typing dropped Return keys

- Reproduction: open KWrite in an isolated display, select its document, type
  `Native contact 17\nDistinct second line\n`, invoke the native Save button, read the file.
- Expected: two distinct lines and the final newline.
- Observed: `Native contact 17Distinct second line\n`.
- Cause: `desktop.py` sent the entire string to `xdotool type`, which did not emit Return keys.
- Repair: send text segments and actual Return keys separately. Faster explicit typing avoids
  the default per-key delay consuming an entire operation timeout.
- Final replay: three KWrite save/readback sequences passed; the retained replay below also passes.

## CA-002 — Clicking an unchanged pointer position timed out

- Reproduction: click the same KWrite document coordinate twice without moving the pointer.
- Expected: the second click completes and produces a new observation.
- Observed: `xdotool mousemove --sync ... 140 100 click 1` timed out after 10 seconds.
- Cause: waiting for pointer movement when the requested position was already current.
- Repair: remove movement synchronization and obtain the actual post-action observation.
- Final replay: two consecutive same-position clicks complete in the retained replay.

## CA-003 — Native text observations hid usable controls

- Reproduction: observe the real Zenity entry dialog and inspect its editable entry/value.
- Expected: the native text control remains discoverable and its current text is returned.
- Observed: the entry and labels disappeared; diagnostic replay exposed
  `Atspi.Accessible.get_text() takes exactly 1 argument (3 given)`.
- Cause: GI's `Accessible.get_text()` interface lookup shadowed `Text.get_text(start,end)`;
  the surrounding catch also discarded the entire node.
- Repair: call the Text interface explicitly; retain node metadata when optional text inspection
  fails. Report sensitivity and editable state from the native state set.
- Final replay: fill `Renée 43 ✓`, read it through AT-SPI, click OK, and independently read
  the program's stdout. All agree.

## CA-004 — Capture could outlive the selected window

- Reproduction: attach to a real Wayland Zenity dialog, invoke OK, and inspect the returned frame.
- Expected: closing the selected dialog must not produce purported fresh visual evidence from
  whatever window becomes active next.
- Observed: the closing-dialog action returned an active-window image of 1280×768 with stale
  dialog metadata. Its capture path did not revalidate the selected process/window after capture.
  The image was deleted rather than retained as valid evidence. No claim is made about its content.
- Cause: compositor active-window capture was not bound to post-capture process identity, and
  disappearing X11 windows could incorrectly enter the host Wayland fallback.
- Repair: verify process birth, active/showing state and dimensions after Wayland capture;
  reject mismatches and delete raw images. Never use host capture for a disappeared isolated
  X11 window. An unavailable image is explicit.
- Final replay: native dialog closure yields no image; isolated capture does not fall back to
  the user's desktop. Arbitrary native Wayland pixel injection remains unsupported.

## CA-005 — Closed apps were rejected before recovery observation/cleanup

- Reproduction: let Zenity exit through its OK control, then snapshot and close its session.
- Expected: bounded observation eventually reports `state=closed`, with no image, and cleanup works.
- Observed: birth checks ran before observation, zombie PIDs appeared live, and stale accessibility
  proxies could throw `The application no longer exists` during final metadata assembly.
- Repair: allow observation/cleanup before action admission; recognize/reap exited processes,
  refresh accessibility caches and handle disappearing metadata without inventing live evidence.
- Final replay: the retained actual-process replay polls to a closed state and completes cleanup.
  A separate 200-ms checker was too strict for asynchronous GUI exit and was corrected to a bounded
  five-second observation window; that timing assumption was a checker defect.

## CA-006 — Real screenshots were not negotiated into developer requests

- Reproduction: submit the saved browser task through the actual Peritus TUI; the writer opens
  the local app and receives the real PNG. First run: `f7c71417e0a84a846ce963c0a96ffba4`.
- Expected: the next model request receives the screenshot as image input.
- Observed: the run stopped with `messages: content requires an unnegotiated capability`.
- Cause: image negotiation considered original attachments, but not images introduced by tool
  observations. The adapter itself and C2 browser launch had succeeded.
- Repair: negotiate optional image input for capable providers and require it when retained
  messages contain images. This also covers images restored by local-context preparation.
- An intermediate repair replay exposed the protocol’s separate rule that required and optional
  capability sets must be disjoint. The final repair excludes required capabilities from the
  optional set; the real run rejected the intermediate overlap rather than accepting it.
- Final replay: actual TUI/provider run `1b861033aaf05e87c4b39d585476784c` completed
  with five registered images, including a fresh independent-reviewer snapshot. The visual-only
  random code matched the separate oracle, and the saved contact matched the service's readback.

## CA-007 — Durable admission rejected tool-generated images

- Reproduction: run the same browser task through the TUI after fixing capability negotiation.
  Run `f7bd7e2071f21428550ab7acb5e87f17` opened the page, then stopped while reserving the next request.
- Expected: the durable provider request includes the host-observed PNG without changing user input.
- Observed: `reserve the provider request in durable control state: The request contains invalid
  conversation or command data`.
- Cause: request admission required every image to equal the exact selected human-attachment list;
  a genuine host screenshot was not represented as a separate observation source.
- Repair: record tool image identity, MIME type, byte count and content digest through a host-only
  callback before request admission. Persist those bindings with the run and used observation sources
  in the request manifest. Explicit user images still must match the exact governing selection;
  unknown/unregistered images remain rejected. Observations grant no task authority.
- Compatibility: existing request/control APIs remain available; older records default to no observed
  images. Empty observation state does not add fields to old-format run records.
- Final replay: the same completed browser run admitted genuine writer and reviewer images into
  durable requests. Native run `2bba3afcb8318685abca4a0875d98472` also admitted 16 real native
  images and produced the exact independently checked file/report; its later review exposed CA-008.

## CA-008 — Repeated hidden native trees exhausted the review context

- Reproduction: ask Peritus through its TUI to create an exact two-line document in isolated
  KWrite, save through the GUI, close the app, and read the file. Run
  `2bba3afcb8318685abca4a0875d98472` produced the correct 45-byte file and JSON report.
- Expected: the independent reviewer can inspect the actual action sequence and outputs without
  being forced to page through irrelevant closed menus for every action.
- Observed: native observations repeatedly contained 500 nodes, mostly hidden menu actions.
  Their complete trees were also copied into the immutable reviewer command brief. After 39
  provider requests and 16 compactions, review stopped with `estimated input tokens 202723 exceed
  provider limit 200000 after deterministic compaction`.
- Repair: collapse and count closed menu branches, retaining refs for scoped expansion. Preserve original observations and
  images, but index native review evidence with exact action arguments, outcome metadata, image
  identity, visible windows and editable values rather than duplicating each complete tree.
  Fresh snapshots remain available to reviewers. No pipeline phase or acceptance obligation is removed.
- Final replay: PASS in actual TUI/provider run `71425c6f3f0ab2055ee09cccb9185f06`.
  The correct native file/report passed independent review with no findings: 22 provider requests,
  20 tool calls and one compaction. Closed-menu filtering produced complete 107–156-node observations.

## CA-009 — Review could not establish ordered workspace steps around GUI writes

- Reproduction: native TUI run `bd32d9e8039991f6eeb2005366ae6c78` declares the proof path,
  creates and saves the exact file through KWrite, closes it, reads the file and writes the report.
- Expected: the independent review can establish the declared-before-write and
  close-before-read-before-report requirements from retained host observations.
- Observed: the reviewer blocked the correct output because its command index began at KWrite
  opening and ended at computer_close. Workspace scope, read and write calls existed in the
  execution history, but were absent from the review source. The fixer repeated an already
  correct GUI workflow; repeating it could not repair the missing host evidence.
- Repair: retain ordered scope/read/write/patch/remove records alongside GUI records. File data
  and patch bodies use digest, byte count and bounded previews; exact observations remain archived.
  The index identifies the originating tool and retains actual computer_close outcomes.
- Final replay: PASS in the same completed native run. The independent reviewer explicitly
  confirmed declaration before GUI writing and close → readback → report order from host evidence.

## CA-010 — Controller lifetime could outlive daemon cancellation/shutdown

- Reproduction: cancel the active native fixer, then request public daemon shutdown with --wait.
- Expected: owned automation resources are cleaned up when their daemon owner exits.
- Observed: shutdown reported unclean with one remaining process; the controller command projection
  still reported running after the daemon became defunct. The disposable scope retained seven
  tasks and required explicit cleanup. No user app was stopped.
- Repair: supply the real daemon PID to the controller. On Linux, retain its birth identity and
  monitor it; owner disappearance or replacement triggers the controller's ordinary SIGTERM/finally
  cleanup, which closes owned apps/displays and detaches existing apps.
- Owner-exit replay: PASS twice through actual KWrite and attached Zenity. The owned KWrite
  process ended, readiness was removed, and the attached Zenity still returned a fresh live
  window observation. Normal final daemon shutdown is checked in the campaign report.

## CA-011 — Isolated Qt apps exported their menus to the host KDE session

- Reproduction: open a real document in isolated KWrite on a KDE host with global menus,
  then try to display and operate its File menu on the private Xvfb display.
- Expected: the owned isolated app keeps its menu controls on that display.
- Observed: the menu bar and File popup remained hidden. Ctrl+M/Alt+F did not show the
  popup, although a scoped accessibility snapshot exposed its offscreen New Window label.
  The inherited session integration exported the menu to the host instead.
- Diagnosis correction: an earlier replay incorrectly treated those accessible labels as proof
  that the popup was visibly open. Inspecting both the selected-window and private-display
  screenshots disproved that checker assumption. The additional full model run
  `155466970fcd56444b446b7c1b33afec` passed the exact file/report and scoped-discovery workflow,
  but its purported menu-opening step is not counted as a visible-popup pass.
- Repair: set KDE_NO_GLOBAL_MENU=1 only for owned isolated child apps. Existing attached apps
  keep their environment. Retain collapsed branch refs and bounded child-name previews, but
  explicitly instruct the model that hidden labels alone do not prove an opened popup.
- Corrected replay: invoke the observed real File menu action, require a showing New Window
  control, scope its popup and inspect the actual image. The keyboard sequence is no longer
  assumed to work. Final direct replay: PASS; the actual captured menu visibly contains New Window.
  Final model replay: PASS in run `1729c2d5872a3333afab5a1e28f03f57`; genuine popup images,
  showing control, report readback and independent review agree.
  The source of the environment option is
  [KDE's environment-variable documentation](https://community.kde.org/Plasma/Environment_Variables#KDE_NO_GLOBAL_MENU).

## Repair locations

| Defects | Changed production source |
| --- | --- |
| CA-001–CA-005, CA-011 | [Native adapter](../../crates/tools/peritus-tools-computer/src/desktop.py) |
| CA-006 | [Model image negotiation and observation input](../../crates/orchestration/peritus-agent/src/developer/execution.rs) |
| CA-007 | [Host image provenance](../../crates/app/peritus-daemon/src/product_control/inputs/observation.rs), [durable request admission](../../crates/app/peritus-daemon/src/product_control/inputs/archive.rs) |
| CA-008 | [Compact GUI review evidence](../../crates/app/peritus-product-runner/src/developer_tools/computer.rs), native adapter |
| CA-009 | [Ordered workspace and GUI evidence](../../crates/app/peritus-product-runner/src/developer_tools/executor/effects.rs) |
| CA-010 | [Daemon owner binding](../../crates/app/peritus-product-runner/src/developer_tools/command_runtime/computer.rs), [owner lifetime and cleanup](../../crates/tools/peritus-tools-computer/src/driver.py) |

## Replay

Build the application normally, then publish the shipped driver and replay real native interfaces:

```sh
cargo build --release --offline --bins --package peritus-cli --package peritus-daemon --package peritus-tui --package peritus-sandbox-linux
target/release/peritus automation setup
python3 docs/testing/reproducers/computer-automation/replay-native-text.py
python3 docs/testing/reproducers/computer-automation/replay-owner-exit.py
python3 docs/testing/reproducers/computer-automation/replay-native-menu.py
```

The browser/model task, predeclared expectations, action log, independent visual oracle and build
origins are retained in `/home/doll/Peritus-computer-automation-2026-10-11/`. That directory contains
ordinary fixture data and actual service/control clients, not a fabricated Peritus UI.
