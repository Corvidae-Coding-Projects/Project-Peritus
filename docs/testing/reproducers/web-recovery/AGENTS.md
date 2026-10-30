# Web recovery replays

Real consumers of the web-source library, owned browser runtime, and public terminal/daemon/provider path.

| File | Responsibility |
| --- | --- |
| [README.md](README.md) | Contracts, build/replay commands, prerequisites and bounded coverage. |
| [read-source.rs](read-source.rs) | JSON-line consumer of the actual public Store API. |
| [replay-sources.py](replay-sources.py) | Seeded HTTP/source paging, restart, classification and cooldown cases. |
| [replay-browser.py](replay-browser.py) | Real browser navigation, challenge and failure cases against disposable HTTP inputs. |
| [replay-product.py](replay-product.py) | Real terminal/daemon/provider default API and rendering recovery. |
| [replay-selected-product.py](replay-selected-product.py) | Real daemon selected-tab read after explicit disposable Chrome selection. |

Keep generated state and binaries outside the repository. Read README.md before executing. These checks establish the observed scenarios, not universal access to protected sites.
