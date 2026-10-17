# crates/tools/peritus-tools-computer/src — navigation

## Responsibility

Optional browser/desktop control host adapters, persistent session RPC, installation/setup, screenshots, observations, and controller driver sources.

## Files in this folder

| File | Role |
| --- | --- |
| [browser.py](browser.py) | Persistent real Playwright browser sessions; no fabricated pages or test assertions. |
| [desktop.py](desktop.py) | Native Linux app accessibility, explicit X11 input, and owned test displays. |
| [driver.py](driver.py) | Authenticated persistent computer-control driver owned by a Peritus C2 process. |
| [lib.rs](lib.rs) | Bounded local RPC, installation cache lookup and setup for computer-control adapters. Entry names: `MAX_RESPONSE_BYTES`, `MAX_REQUEST_BYTES`, `installation_root`, `python`, `prepare_driver`. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
