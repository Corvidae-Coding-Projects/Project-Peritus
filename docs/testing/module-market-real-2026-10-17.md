# Native Rust module market — real testing, 2026-10-17

## Contract and boundaries

Peritus must discover, install, enable, and select independent **native Rust code
plugins** through TUI/GUI menus. New executable modules must work without compiling
feature code into Peritus. Deep Research belongs to its own market crate and owns
its coordinator, evidence ledger, report generation, role schemas, and checkpoints.
The host supplies generic process IPC, provider access, permitted services, cancellation,
artifact paging and action presentation. Existing declarative workflows remain usable.

Before running the campaign, expectations were recorded in `.design/module-market.md`:
exact remote package identity and disabled installation; explicit enable/use;
no pre-Start source gathering, stale Start refusal, no implicit Resume approval;
actual primary-source research through independent role contexts; source-bound report
readback; restart with retained stages; explicit Design attachment; ordinary Chat;
and a second independently compiled Rust extension that adds native functions.

Use isolated XDG state and ordinary disposable folders. Preserve live user daemons.
Build actual binaries with `cargo build --locked --release --jobs 2 -p peritus-cli
-p peritus-daemon -p peritus-web`, and `npm run build` in webui. No unit tests, mock
backends, formal checks or repository verification suites are used.

## Completed evidence

- Native module package published at market commit `40f09a47fccd4050b335b2c1bc5a203a7b45f2cb`.
  Actual GitHub refresh/install bound digest
  `e6f1de46ccf298c01e3508e8cf5392fb3fb6129ca36d12a4574bc1678301ebaf`,
  read back its matching source, executable and origin commit, and installed disabled.
  Receipts: `/tmp/peritus-native-market-remote-catalog.json` and
  `/tmp/peritus-native-market-remote-install.json`.
- Independent executable public-library consumer: 24 varied research journeys,
  12 catalog sizes, three maximal-source-page shapes and 27 fresh-process readbacks.
  Receipt: `/tmp/peritus-market-research-native-library-final/receipt.json`.
- Actual independently compiled native Rust executable over framed IPC: three complete
  role/correction workflows, stale plan/request-key rejection, independent source
  admission, exact report checking, fresh native process resume and attachment readback.
  Receipt: `/tmp/peritus-deep-research-native-rpc-delivery/receipt.json`.
  Executable SHA-256 `79916093180b0a89fcb23e8adffc9474621c1220e24f9de55e378c31a12cb960`.

The public-library and native IPC consumers provide explicit host observations/model
results as API inputs. They establish the actual plugin code's behavior and persistence,
not HTTP, live-model or human UI behavior. Those paths are recorded separately below.

## Confirmed defect and replay

A legal web-tool page of about 1 MiB expanded past the research ledger's original
1 MiB per-record serialization bound. All three independently generated ASCII,
JSON-escape-heavy and Unicode cases failed on the original implementation.
Original receipt: `/tmp/peritus-market-research-consumer-before-boundary-fix-confirm/receipt.json`.
The Rust ledger now stores exact overlapping 64 KiB UTF-8 records, preserving exact
16 KiB quotes across chunk boundaries. All three shapes and fresh-process quotation
readbacks pass in the native-library and IPC replays above. The generic IPC envelope
allows the extra JSON escaping of a legal source page, while retaining per-transfer bounds.

Retained public-library and native executable replays:
`reproducers/market-research-consumer.rs`, its `.md` build guide, and
`reproducers/native-research-rpc.py`. Source-rooted safeguards for pre-Start observations
and actual-report inspection also pass; no pre-fix execution is claimed for those two.
An initially incorrect Markdown comparison in the IPC driver was corrected to account
for blockquote line prefixes; the exact underlying candidate bytes were checked separately.

A final-host CLI and actual TUI replay also exposed rejection of the standard
`x86_64-unknown-linux-gnu` artifact target, because target validation admitted only
alphanumeric characters and hyphens. The failed CLI receipt is
`/tmp/peritus-counter-extension-acceptance/manifest-underscore-failure.json` (exit 70);
the actual market-panel error is retained under
`/tmp/peritus-native-market-live-final-20261017/terminal-screen.txt`. Target validation
now accepts underscores alongside hyphens and alphanumeric bytes. Final CLI/UI
replay evidence follows below.

## Application campaign

Final application binaries: CLI SHA-256 `6604d42b0690b17c08f1a0a57839cfbc49859a46d76aa3304b5fabb598cc22cb`,
daemon `d361bf783d9d6cab58b1e8431e436e69f81dee1a41d7383942ddec959f1289b2`,
and gateway `170f1752c90524bff7127d8e89a06f12ce23a42a2493add18cb953cee128c14c`.
Build log: `/tmp/peritus-native-module-delivery-build.log`; browser assets built with
`npm run build` (`/tmp/peritus-native-module-web-build.log`). All app mutations were
through actual CLI/TUI/GUI controls against isolated state and a plain folder.

| Journey | Result and independent observations |
| --- | --- |
| Independent Rust extension | PASS. Unmodified native Rust template compiled separately. Twelve Unicode/quote/multiline/empty inputs matched independent Python scalar/word/UTF-8 expectations through actual IPC; real CLI install/enable/load/list read back identical executable bytes. Receipt `/tmp/peritus-counter-extension-acceptance/corrected-install.json`, RPC receipt `rpc-output/receipt.json` under that directory. |
| Extension through Peritus | PASS. Three direct TUI commands returned exact expected JSON counts. Two distinct real-provider requests invoked the plugin's exported `text_count`, returning distinct correct six/eight-scalar observations. No counter feature code was added to Peritus. |
| Native market TUI | PASS. Real remote refresh, row selection, installation disabled, explicit enable and editable `/module ID` selection. Actual client actions and durable records are retained below. |
| Plan gate and scope revision | PASS. Actual provider saved a plan with no material unanswered questions; wrong digest and old-revision Start were refused. Resume did not approve the plan. No source observations existed in either new episode before valid Start. |
| Technical research | PASS. Real web reader opened the official Rust Book, gatherer saved exact passages, separate skeptic context admitted qualified documentation claims, synthesis saved a typed report, final skeptic inspected the actual draft and reread its cited ranges. The checked answer and String-specific qualification were manually compared against the original Rust Book. |
| Design and ordinary Chat | PASS. Explicit Use in Design snapshotted the exact report; real Design saved a private draft about the attached ownership research. No implementation started. Subsequent ordinary Chat returned the requested marker. |
| General research and recovery | PASS. Real keyless search and NASA page reading retained an orbital-year passage. Brave returned the original NASA result; Yahoo reported a redirect failure and DuckDuckGo/Startpage were empty. This does not establish all search engines as healthy. Stop after committed gathering left phase 2 intact. Fresh daemon/client reattachment resumed assessment and finished source-checked synthesis. The resulting qualified NASA claim was independently compared with NASA's page. |
| Generic TUI actions and legacy path | PASS. A fresh client used plugin-supplied numeric plan/report actions. An existing declarative one-stage package read its declared Unicode resource and returned the exact marker through a live provider. Receipt `tui-menu-evidence.json`. |
| GUI native package and report | PASS. Real Chromium GUI filtered/refreshed the market, disabled/removed/reinstalled the actual native package, explicitly enabled it, prepared the generic command, opened an actual previously checked report, and attached it for Design. |
| GUI prepare, Start and Stop | PASS. A fresh real-provider plan had no pre-Start gathering. The plugin-supplied Start button launched actual source collection. Stop preserved committed gathering and left the deliberately unfinished episode at phase 2. No browser page errors were observed. |

Application artifacts are under `/tmp/peritus-native-market-delivery-20261017`:
`terminal.ansi`, durable daemon records, exact private source/admission/report files,
two checked report readbacks, `tui-evidence.json`, `tui-menu-evidence.json`,
`browser-evidence.json`, `browser-native-plan.png`, `browser-native-checked-report.png`
and `browser-native-start-stop.png`. Initial assertions and stop output are in
`/tmp/peritus-native-market-delivery.log`; successful retained-state completion is in
`/tmp/peritus-native-market-resume-final.log`. GUI completion is recorded by
`browser-success` and the actual action receipt. The active user daemon was preserved;
owned test clients, gateways and daemons shut down cleanly.

Driver corrections are distinct from product defects. The terminal parser's wide-cell
display bug needed a parsed-cell fallback while retaining raw bytes. Stop is labeled
Cancelled, not only Stopped. Numeric action replay must observe UI idle state. A GUI
attempt found an older identical reply before the latest operation had settled; the
application correctly refused the subsequent action as active and retained its draft.
The final driver waits for a new received input and the matching visible UI state.
The failed attempt remains as `browser-attempt1-evidence.json` and corresponding
screenshots. One disposable replay was interrupted while adjusting its driver; its
additional recovery also preserved state. These are not counted as application bugs.

Replay the TUI campaign with `reproducers/module-market-live.py --binary
/path/to/peritus --provider-state /path/to/nonsecret/provider-state.json --output
/FRESH/temporary/directory --counter /path/to/compiled/template/package`. Use the
retained consumer guide to build the native Rust template and public consumers.
After `browser-ready`, run `module-market-menu.py BINARY CAMPAIGN`, then
`PERITUS_PLAYWRIGHT=/path/to/playwright/index.mjs node
reproducers/module-market-browser.mjs http://127.0.0.1:4267 CAMPAIGN` from repo root.
`--resume-after-stop` continues retained stopped fixture state; `--serve-existing`
opens an idle retained fixture for GUI replay. They preserve existing inputs.

No unresolved functional mismatch was observed in these bounded journeys. An existing
presentation limitation remains: a role's streamed final prose can also appear in the
host's final reply, repeating a sentence. Optional absent draft/review reads produce
correctable not-found observations during initial preparation; completed work and final
checking progressed despite them. Neither is evidence of a failed research result.

The final source was also rebuilt independently from the self-contained market
workspace with the pinned Rust toolchain (`/tmp/peritus-market-native-source-build.log`).
Formatting changed the binary's source-location metadata without changing feature
logic or the embedded role/policy text. The final packaged executable SHA-256 is
`de1ce375956bbe32c2467d17c03075906d31f3eea2251edc23b8e641dd75d996`;
package digest is `72ce67465575700bd73143ee0f0cfe1f202fb577074b438b5de541744407c209`.
It is published in market commit `8e965c6`. Its actual framed IPC replay passed
159 checks across three complete native workflows, maximal pages and durable process
restarts (`/tmp/peritus-native-final-source-rpc/receipt.json`). The full application
campaign above used the earlier behavior-identical pre-format executable; no extra
full provider campaign is claimed for the source-location-only rebuild. Final remote
installation/host launch/provider-plan replay also passed against that final published
package: `/tmp/peritus-native-final-source-plan-20261017/final-source-plan-receipt.json`.
It fetched commit `8e965c6d0e167ea27b8d78d0d20849fef0828ef7`, installed disabled,
explicitly enabled, launched the native help through the actual host, and used the
real provider to save a fresh plan with no pre-approval gathering. The same tested
CLI/daemon hashes are recorded in that receipt.

## Limits

This campaign runs Linux x86-64 GNU. It does not establish native macOS/Windows,
PDF/scanned/script-rendered extraction, arbitrary source truth, or multi-hour reliability.
A source-checked AI assessment is not a guarantee of factual infallibility.
