# Stage 7 web source and browser boundary

The `web_source` target opens actual loopback HTTP pages through `peritus-tools-web::Store`.
Generated text includes UTF-8 multibyte characters, combining marks, ZWJ sequences and
newlines. An independent byte model checks exact page slices, byte continuations, line numbers,
SHA-256 identities, literal search positions and result continuations. Each case exercises an
optional redirect, an immediate cache hit, pre-fetch cancellation, an explicit refresh, retrieval
of the original immutable source, a forced-expired cache index when selected, invalid byte
offset rejection, a redirect to HTTP 429 with `Retry-After`, an unrecognized HTTP 599 status,
and rendered non-success rejection. Failed responses must not acquire a successful cache index.
A loopback server counts actual network
requests, so cache expectations are not inferred from returned flags alone.

The fixed calibration case requests a seven-byte page. The unapplied calibration patch raises
the production minimum page size to eight bytes; exact replay and a generated search must detect
the mismatch after a successful mutant build, then pass after source restoration. A source edit
without rebuild must be reported as incomplete by the harness fingerprint gate.
This calibration completed: stale replay exited 2, rebuilt mutant fixed replay exited 1,
seed 19427 generated and minimized a mismatch in 80 checker invocations, and both the fixed and
minimized cases replayed with exit 0 after exact source restoration. The original web source
file has no remaining diff. See `runs/stage7-web-source/{mutant-stale,mutant-replay,mutant-search,
restored-fixed-replay,restored-minimized-replay}` for receipts and witnesses.

The `browser.py` driver and `driver.py` screenshot path were inspected and exercised with the
installed managed Playwright runtime and a disposable loopback Chromium profile. Navigation,
tab close, fresh observation, direct status reporting and redirect status reporting were
observed through the real browser interface. Sensitive owner findings and candidate fixes are
kept outside this public discovery bundle for separate review.

`stage7_browser_smoke.py --revision d106c9b13318fbcdd3d816ed63ae48532806fa7e` reconstructs
the sealed browser-driver sources from Git and runs eight safe loopback checks. The resulting
`runs/stage7-web-source/browser-smoke.json` records Linux, Python and Playwright versions and
the exact SHA-256 of both driver sources. This avoids depending on later edits to the separate
original checkout. That checkout's browser source has since changed independently, so the
sealed-source receipt does not qualify its current behavior.
The sealed driver reported a redirected HTTP 429, HTTP 599, and two known HTTP 200 challenge
forms as successful readings with retained files. An isolated Stage 7 candidate rejects
these before writing a reading; `browser-fixed.json` passed all eight checks with
`--assert-failed-pages`. The original checkout has a broader independently edited status
change; it was not copied here. See `findings/stage7-browser-http-status.md`.

Coverage limits are explicit. The source target does not call a public search engine, external
HTTPS site, product-run permission gateway or installed browser extension. This checkout
contains the tracked Python Playwright driver and native Rust host adapter; no Chrome extension
or native-messaging bridge asset was found in tracked or ignored files in this restored worktree.
Consequently no replay depends on an ignored bridge file. Selected HTTPS origin consent has no
direct test here. The candidate recognizes the two tested challenge forms but does not
classify every possible successful-status challenge or unusable page. Browser status
observations are not substitutes for retained Rust source checks until the cross-language path
is exercised together. The target checks that `Retry-After` is preserved in the HTTP error; it
does not demonstrate a scheduled retry or elapsed backoff. Native browser automation on other
operating systems remains unmeasured.
