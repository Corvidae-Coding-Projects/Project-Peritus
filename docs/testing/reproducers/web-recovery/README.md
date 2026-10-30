# Web recovery real-interface replays

The independent expectations are: HTML bodies and code remain readable; a challenge that navigates
403 -> 200 returns the final document's status; known challenge HTML at 200 is unsuccessful;
ordinary access denials do not trigger browser retries; rate limits suppress requests until the
cooldown expires without blocking another origin; retained UTF-8 pages reconstruct the exact digest
across fresh processes; explicitly selected HTTPS DOM reads preserve unknown HTTP status and lose
access on revocation without closing the user tab.

Build the normal application with `cargo build -p peritus-cli -p peritus-daemon`. Build
`read-source.rs` as a standalone Rust 2024 consumer against the resulting `peritus_tools_web` and
`serde_json` rlibs, using `-L dependency=TARGET/debug/deps` and the exact artifacts emitted by
`cargo build --message-format=json`. It is deliberately outside workspace membership.

Run the source cases with:

```sh
python3 docs/testing/reproducers/web-recovery/replay-sources.py --consumer /absolute/read-source --output /temporary/source-evidence
```

This uses seed 7319, eight varied Unicode documents, eight distinct failure/cooldown cases, real local
HTTP services, exact retained pages and fresh consumer processes. No external API or substituted
HTTP implementation is used by this replay. The ordinary HTTP inputs are explicit fixture data.

Run the browser cases using the provisioned Playwright Python:

```sh
/absolute/automation-python docs/testing/reproducers/web-recovery/replay-browser.py --source /absolute/checkout --output /temporary/browser-evidence
```

The six cases cover challenge navigation, forbidden/authentication/rate-limit responses, script
execution and a challenge body returned at status 200. It closes only its own browser processes.

`replay-product.py` takes `--binary`, `--provider-state`, `--automation-python`, `--browser-cache`
and `--output`, following the existing web reader replays. It requires `pexpect`, `pyte`, a signed-in
provider and normal native sandbox prerequisites. It creates a fresh terminal/daemon/workspace,
submits the actual default `web_open` requests, checks the retained tool observations, and shuts
down only its exact disposable daemon. It reads one real public Stack Overflow thread via its
official API as well as local shell/challenge/denial pages. Do not poll equivalent API requests
more than once a minute; observe returned quotas and backoff.

`replay-selected-product.py` uses the same options plus `--selection` JSON with `session`, `url`
and `data` fields. The session must already belong to an explicitly shared disposable HTTPS tab
and `data` must be that disposable Chrome bridge's XDG data directory. It checks the real product
`web_open browser_session` result. Never use an unrelated user's browser profile as fixture data.

For Chrome UI qualification, use a disposable headed Chrome-for-Testing profile and its actual
private native host. Observe **Share this HTTPS tab** in the toolbar popup, read through the bridge
and product path, reject unshared tabs/external HTTP/interaction, then use **Revoke this tab** and
verify that the selected session becomes unreadable while the tab remains open. Production
CAPTCHA completion and signed-in private-site access require the user's own normal interaction;
a public example does not establish universal unblockability.

The 2026-10-29 campaign receipts and exact Chrome UI driver are retained outside the repository at
`/home/doll/.cache/peritus-web-recovery-_94c9pm_/`; this is local evidence, not a required runtime path.
# Chrome open/share diagnostics

`replay-extension-open.py` installs the real embedded extension into an owned Chromium profile
and communicates through its real native-messaging bridge. Pass `--cli`, `--driver-dir` (the
production Python driver sources), `--browser` (the Chromium executable), and a fresh private
`--output` directory. Run it with the installed automation Python; Xvfb must be available.
No existing Chrome profile or desktop clipboard is used.

Opening `https://doc.qt.io/` without sharing the new tab must preserve
`browser_tab_selection_required`, the opened URL, and the popup share instructions. Before
the repair, `--expect-old` reproduces the generic `Chrome extension action failed` exception
which discarded those instructions. The same replay requires a successful loopback read and
rejection of external HTTP. It never grants access to an unselected HTTPS tab.
