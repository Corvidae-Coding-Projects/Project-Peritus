# crates/tools/peritus-tools-computer/src — navigation

## Responsibility

Browser/desktop control host adapters, persistent session RPC, installation/setup, screenshots, observations, and controller driver sources.

## Files in this folder

| File | Role |
| --- | --- |
| [browser.py](browser.py) | Persistent real Playwright browser sessions; no fabricated pages or test assertions. |
| [desktop.py](desktop.py) | Native Linux app accessibility, explicit X11 input, and owned test displays. |
| [driver.py](driver.py) | Authenticated persistent computer-control driver owned by a Peritus C2 process. |
| [setup.rs](setup.rs) | Managed per-user runtime setup, actual browser readiness, first-launch repair and Linux library installation. |
| [setup/](setup/AGENTS.md) | Pinned platform bootstrap downloads and executable publication. |
| [lib.rs](lib.rs) | Bounded local RPC, installation cache lookup and setup for computer-control adapters. Entry names: `MAX_RESPONSE_BYTES`, `MAX_REQUEST_BYTES`, `installation_root`, `python`, `prepare_driver`. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
