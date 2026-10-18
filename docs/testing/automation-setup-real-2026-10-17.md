# Automatic browser runtime installation — real testing, 2026-10-17

## Requirement and cause

A fresh Peritus installation must prepare its browser runtime automatically on supported Linux,
macOS and Windows systems. It must not assume the user has Python, pip, Playwright, Chromium,
or a separately configured automation environment.

The live neutral-topic Deep Research run exposed `Failed web_open: No module named 'playwright'`.
The original automation resolver fell back to system Python when its optional per-user venv
was absent; native installers never called automation setup. Native HTTP/search still worked.
The clean fixture correctly exposed an unprepared rendering path rather than a missing module
binary. Its source observations remain in `/tmp/peritus-deep-research-ea-20261017`.

## Change

Native installer adapters call the actual `peritus automation setup` before reporting success
on Linux, macOS and Windows. Their existing upgrade paths reuse those installers. Portable,
source and system-package interactive launches automatically prepare missing per-user state;
a download error is shown without disabling ordinary coding, and a later launch retries.

Setup downloads a version-pinned, embedded-checksum-verified native uv archive for all six
OS/architecture combinations, then creates a private runtime using compatible existing Python
or managed Python. It installs Playwright 1.64.0 and Pillow 13.0.0, downloads Chromium, launches
it and executes JavaScript before atomically publishing readiness. Setup calls serialize with
an OS file lock. Existing legacy runtimes remain intact; an explicit Python override remains
operator-owned. Linux library repair uses supported system package managers only when actual
browser launch reports missing libraries. The existing dependency-install opt-out is honored.

The bootstrap version is uv 0.12.22. Its six archive identities were checked against the
[upstream release](https://github.com/astral-sh/uv/releases/tag/0.12.22). Managed Python follows
[uv's documented behavior](https://docs.astral.sh/uv/concepts/python-versions/). Browser OS
requirements remain those of [Playwright](https://playwright.dev/python/docs/intro).
The compatible existing-interpreter check requires Python 3.11 or newer because
[Pillow 13's metadata](https://pypi.org/project/pillow/13.0.0/) requires it. An older interpreter
causes managed Python selection; an incompatible environment left by a failed setup is rebuilt
inside the versioned managed directory. Legacy environments remain untouched.

## Actual operation

Built CLI and daemon with `cargo build --release -p peritus-cli -p peritus-daemon --jobs 2`.
Only changed Rust sources were formatted. No unit, mock, formal, qualification or repository
verification suites ran. The live builder daemon and personal browser were not modified.

| Scenario | Observation |
| --- | --- |
| Clean user data and no Python on PATH | PASS: downloaded private uv and CPython 3.13.16, installed both libraries and Chromium, launched Chromium 156.0.8078.4 and executed JavaScript. The venv base is inside this fixture's managed Python directory. |
| Existing Python 3.10 and failed setup recovery | Initially FAIL: actual Python 3.10.22 was selected and Pillow 13 could not resolve. Corrected compatibility selection. Final binary replayed the same failed environment, downloaded managed Python 3.13.16, rebuilt only its managed environment, installed the packages and successfully launched Chromium. Fresh-process status selected that repaired runtime. |
| Real download output | Initially FAIL: dependency progress polluted CLI result stdout. Fixed child progress routing to stderr. Final replay removed only the fixture's small FFmpeg cache, downloaded it again, and parsed the entire stdout as one JSON result. |
| Two concurrent setup processes | PASS on final binary: both completed through real setup, reported installed dependencies, and retained identical readiness content. |
| Fresh-process status | PASS: a new CLI selected the managed interpreter and found the installed browser and libraries with system Python still absent from PATH. |
| Real browser/controller | PASS: three dynamic-page reads with different waits using bundled Chromium and system Chrome hidden from PATH. Real DOM contained the generated Unicode canary. File URL rejection and an independently open browser session also remained correct. |
| Linux native installation and upgrade | PASS: actual canonical scripts installed/upgraded a disposable user directory and invoked automatic browser setup. Installed executable bytes matched their development bundle. This was a locally assembled development bundle, not a published release package. |
| Automatic first-launch repair | PASS: removed only this campaign's readiness pointer, then launched the actual interactive client without a manual setup call or interpreter override. It rebuilt readiness and connected normally. |
| Existing runtime and native accessibility | PASS on final binary: a disposable installation based on the existing Python 3.14.8 runtime retained that runtime unchanged, imported `gi.repository.Atspi` in the new environment, and reported native accessibility available. This checks dependency compatibility, not a new desktop interaction journey. |
| Live provider/TUI rendered read | PASS on final binary: actual Chat called `web_open render=true` against a real HTTP page. Its retained `method=browser` source contained `AUTOMATIC-RUNTIME-7429-水`, which appeared in the DOM only after JavaScript ran. No workspace file was changed. |

A packaging-driver mistake was corrected separately: source-checkout shell scripts lack the
executable package modes. The first constructed upgrade fixture therefore failed to execute
its installer. Applying the canonical manifest's executable modes made the actual upgrade
pass. No product change was made for that invalid fixture.

## Evidence and replay

The compact [evidence record](automation-setup-evidence.json) retains final executable hashes and observed rendered text. Raw observations are retained in:

- `/tmp/peritus-automation-auto-20261017/receipt.json`: final setup, repair, concurrent calls,
  managed Python configuration, fresh status; initial stdout failure and corrected stderr/output.
- `/tmp/peritus-auto-render-20261017/evidence.json`: actual browser/controller observations.
- `/tmp/peritus-auto-python310-20261017/before.json` and `after.json`: actual incompatible-base
  failure and final managed-interpreter repair, including version/configuration and fresh status.
- `/tmp/peritus-auto-installer-final-20261017/receipt.json`: final native installer/upgrade and installed hashes.
- `/tmp/peritus-auto-legacy-final-20261017/receipt.json`: final unchanged legacy runtime and native accessibility imports.
- `/tmp/peritus-auto-launch-final-20261017/receipt.json`: final CLI/daemon hashes, automatic launch
  repair, exact human input and the actual rendered source from the live provider journey.

Cold-install replay on Linux, using fresh disposable state:

```sh
case_root=$(mktemp -d /tmp/peritus-automation-replay.XXXXXXXX)
XDG_DATA_HOME="$case_root/data" XDG_CACHE_HOME="$case_root/cache" \
  target/release/peritus automation setup
XDG_DATA_HOME="$case_root/data" XDG_CACHE_HOME="$case_root/cache" \
  target/release/peritus automation status
```

To exercise absent-Python behavior, use a private PATH containing the ordinary extraction
helpers (`tar`, `gzip`, and their shell dependencies) but no Python. Invoke the Peritus binary
by absolute path. The retained cold driver is `/tmp/peritus-runtime-setup-live.py`; final
focused replay is `/tmp/peritus-runtime-final-live.py`. Both invoke the actual application.
The installed interpreter can run the retained controller replay:

```sh
PLAYWRIGHT_BROWSERS_PATH=/path/to/fixture/cache/ms-playwright \
python3 docs/testing/reproducers/web-browser.py \
  --python /path/to/fixture/data/peritus/automation/runtimes/playwright-1.64.0-pillow-13.0.0/bin/python \
  --output /tmp/peritus-automation-browser-replay --chromium-only
```

## Limits

Native execution was Linux x86-64. macOS, Windows, ARM64 and system-library installation on a
host actually missing graphical libraries were not executed. Their installer paths and pinned
upstream artifacts were inspected; they are not counted as native passing tests. The cold
provisioning run preceded the stdout and interpreter-selection corrections; the final binary
replayed actual download progress, incompatible-interpreter recovery, concurrency,
installer/upgrade, first launch and live TUI rendering.
No local user binaries were reinstalled or live user jobs restarted for this campaign.
