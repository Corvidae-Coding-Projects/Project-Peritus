# Property campaign navigation

`peritus-praxis` is a nested, optional Rust workspace. Production packages remain the subjects; property models and generated histories live here. The root Cargo workspace and release dependency graph do not include this package.

| Area | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml), [Cargo.lock](Cargo.lock) | Pinned property-runner dependency graph. |
| [src/](src/) | Public API strategies, independent models, exact replays, and source fingerprint checks. |
| [owner/](owner/), [support/](support/) | Test-only private owner consumers and disposable fixtures. |
| [contracts/](contracts/) | Oracle definitions, caller paths, and tested limits. |
| [campaign.py](campaign.py), [owner.py](owner.py) | Bounded build, search, replay, and process controllers. |
| [stage7-web/](stage7-web/) | Browser-client state and recovery property drivers. |
| [stage7-launcher/](stage7-launcher/) | Provider discovery and release-version property drivers. |
| [stage8-policy/](stage8-policy/README.md) | Release evidence, quality acceptance, gate report, and cross-system settlement properties. |

Build the current source before attributing a result. A mutation must compile and make a saved case fail for the intended behavior, then be reversed. Keep generated cases, logs, and archives outside the committed source. See [README.md](README.md) for current commands and limits.
