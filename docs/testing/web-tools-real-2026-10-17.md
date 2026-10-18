# Native web tools: real testing, October 17, 2026

Implemented `web_search`, `web_open`, `web_read`, `web_find`, native HTTP reading, immutable source references, keyless SearXNG setup/status/stop, and optional isolated browser rendering. Conversation, Design, writer/fixer and reviewer catalogs share the provider-neutral tools. The full deep-research coordinator is not part of this change.

## Artifact and method

Started from develop `b13d6f2bd436bd77fc06665e0b736583b4490fa1`. Built the actual CLI and daemon with:

```sh
cargo build --release -p peritus-cli -p peritus-daemon --jobs 2
```

Final CLI SHA-256: `c46a680346a661353e3db1dc128c63d01a0a475eb0d79781fe7707345652234c`.
Final daemon SHA-256: `fb39bf1cebd09312291dc14deee87baf89a630438702fdf2eec8ea6f8f6ba0c4`.

Used isolated XDG directories and a disposable packaged CLI/daemon/TUI under `/tmp/peritus-web-real-20261017`. Kept the existing signed-in official Codex runtime and browser executables; did not copy credentials, replace the local installation, or interrupt the builder user's live daemon. No unit suites, mocks, formal verification, or repository verification suites were run.

Local HTTP pages are ordinary independently specified fixture documents served over real sockets. Search used actual SearXNG and upstream engines. Rendering used actual installed Chrome and Playwright Chromium. Terminal journeys used the real daemon and official Codex provider, then checked retained tool observations rather than accepting a model's assertion alone.

## Observed results

| Journey | Observed result |
| --- | --- |
| Native generated campaign | PASS: 29 scenarios, 10 generated Unicode text documents and four generated HTML documents; 133 exact text pages reconstructed across fresh CLI processes. Largest text document: 3,148,008 bytes. Seed `20261017`. |
| HTML extraction | PASS: nonempty title/body, entities, relative/base URLs, preformatted indentation, script/style exclusion, and tokens after 70,000 source lines. Case-insensitive HTTP media/charset parameters work. |
| Retention | PASS: cache hit avoids another HTTP request; refresh returns updated text while the previous reference remains readable. Eight fresh clients obtain coherent immutable content. Corruption, bad references and invalid UTF-8 offsets are rejected. |
| Transport and rejection | PASS: gzip, UTF-8, Latin-1 and UTF-16; redirect, redirect loop, unsafe file redirect, 404, 429/Retry-After, unsupported PDF, invalid bytes, credential-bearing URLs and the declared transfer-size guard. No truncated document is reported as complete. |
| Real public HTTPS | PASS: opened the Rust Book ownership page, extracted its title/body, found ownership passages and read the matching ranges. |
| Real local search | PASS: managed SearXNG readiness, loopback binding, repeated setup, stop/restart, JSON queries, immediate query cache reuse and actual upstream results. Default Yahoo/Mwmbl results included official Rust Book URLs, which were opened through the CLI. |
| Search failure isolation | PASS: external endpoint configuration; refusing to stop an external service; unavailable search diagnostics; known URL reads continue without search. |
| Actual model/daemon round trip | PASS: workspace listing/read, web search/open/find/read and JavaScript rendering in a terminal conversation. The rendered source contained `Compass 7821: JavaScript arrived`; the HTML fixture's initial body contained only a waiting message. |
| Read-only roles and permissions | PASS: Review and guided Design called `web_open` and read the actual `WAYPOINT-9814` heading. With Process restricted, rendered open returned a permission error and native HTTP succeeded. The retained trace contains both outcomes. |
| Both browser installations | PASS: three renders using installed Chrome and three with system Chrome hidden from PATH, using installed Playwright Chromium. File URLs were rejected; an independently opened browser session remained observable after the reads. |
| Transfer cancellation | PASS: public Rust-library consumer canceled before a request and during a slow real transfer, then successfully fetched/found a second document. The server observed exactly one slow request and one recovery request; measured recovery sequence 109 ms. |
| Wedged browser recovery | PASS on final code: a real page entered an infinite JavaScript loop after navigation. The reader returned its bounded RPC error, its canceled controller PID exited, and the same run created a fresh reader and rendered the healthy page. The final trace contains the expected failure followed by a successful `method=browser` source with the independent Compass value. |

SearXNG ran from the official `docker.io/searxng/searxng:latest` image, reporting `2026.10.17-2a6fc07be`. The managed instance used two CPUs, 512 MiB memory, one worker and a PID bound; a sampled instance used about 83 MB. Upstream Brave rate limits, DuckDuckGo CAPTCHA and Google access denial were retained in response metadata and reported by the model. These are partial upstream failures, not passing evidence for those engines. Yahoo and Mwmbl supplied relevant results on this network. Ranking varies; snippets remain discovery evidence and primary pages must be opened.

## Confirmed defects found and repaired

| Defect | Reproduction / original observation | Cause and final replay |
| --- | --- | --- |
| HTML reads returned empty content | Real redirected fixture and the live Rust Book returned HTTP 200, empty title/body and zero links. | Moving the RcDom root into the traversal allowed its destructor to clear descendants. Keeping the root owner alive fixed both actual pages. The retained CLI replay fails on the empty-body implementation. |
| Web reader ignored an available system browser | Actual Peritus rendered open failed with missing bundled Chromium, while the existing browser-control launch supports installed Chrome. | The new path used a different executable-resolution strategy. It now uses the existing system-browser fallback and explicitly forwards the host browser-installation cache into the owned runtime. Both real installations were exercised. |
| Initial local search engines produced no useful search | Initial defaults returned zero results with CAPTCHA/rate-limit failures; `keep_only` also preserved disabled engine defaults. | Explicitly enabled the chosen keyless engines and added Yahoo/Mwmbl after live comparisons. The final generated configuration returned official documentation and still reported partial failures. |
| Canceled web reader stayed selected | A wedged-page replay returned cancellation pending; later healthy renders repeatedly received “Computer controller stopped… Start a new run.” | Web reads now own a separate replaceable controller. The host polls cancellation, clears terminal ownership and replaces a stopped reader within the same run. Final real terminal replay successfully rendered the healthy page after the timeout. |

Driver issues were kept distinct from product defects: an early terminal completion condition matched text still in the composer and was discarded; the retained recovery driver checks actual tool trace outcomes. An unsafe `file:` redirect is rejected as HTTP 302 by reqwest without following it, so the replay expects that rejection rather than assuming a particular lower-level error string.

## Retained replays

All output directories must be new. The CLI replay needs the built executable, not a daemon or provider:

```sh
python3 docs/testing/reproducers/web-tools.py \
  --binary target/release/peritus --output /tmp/peritus-web-cli-replay
```

Real browser/controller replay, with a real installed automation interpreter and browser cache:

```sh
PLAYWRIGHT_BROWSERS_PATH=/home/doll/.cache/ms-playwright \
python3 docs/testing/reproducers/web-browser.py \
  --python /home/doll/.local/share/peritus/automation/runtime/bin/python \
  --output /tmp/peritus-web-browser-replay
# Add --chromium-only to hide system Chrome and exercise installed Playwright Chromium.
```

The stalled-reader replay requires Linux, `ss`, an accessible packaged CLI/daemon/TUI, and a signed-in provider selected in an existing nonsecret product-state JSON. It creates a separate state root, drives the actual terminal, checks actual trace frames and shuts down only its disposable daemon:

```sh
uv run --with pexpect --with pyte python docs/testing/reproducers/web-reader-recovery.py \
  --binary /path/to/package/bin/peritus \
  --provider-state /path/to/product-state/state-NNN.json \
  --automation-python /path/to/automation/runtime/bin/python \
  --browser-cache /path/to/ms-playwright \
  --output /tmp/peritus-web-reader-recovery
```

`web-native-consumer.rs` is a minimal consumer of the real public C4 library. Build it against the current `peritus_tools_web` and `serde_json` rlibs, then pass a disposable store directory, a slow HTTP URL and a normal URL whose body contains `AFTER-CANCELLATION`. It checks cancellation against the real transfer and recovery through the same public API.

Detailed disposable evidence is under `/tmp/peritus-web-native-complete-20261017`, `/tmp/peritus-web-real-20261017`, and the browser replay output directories. The saved replay drivers preserve the useful input classes without committing large successful transcripts.

## Coverage boundaries

Native execution was Linux x86-64. macOS/Windows paths and Docker were inspected but not executed; Podman supplied the real local search service. Actual provider inference used Codex; other adapters were not modified or separately exercised. No hours-long research campaign, natural TTL expiry, 128-entry eviction boundary, PDF extraction or full deep-research workflow is claimed. Permission routing was exercised for Process denial; the existing Network/Read admission mapping was reviewed, not separately raced against an in-flight provider call. This is bounded behavioral evidence, not exhaustive application coverage.
