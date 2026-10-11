# peritus-network

`peritus-network` narrows a checked C2 network contract into a digest-bound runtime plan and owns a
bounded loopback HTTP/CONNECT proxy. It rechecks names, resolved addresses, redirects, limits, and
credential scope. The crate grants no authority and never interprets a proxy decision as run
acceptance.

The proxy is deny-by-default, has bounded workers and observations, and joins owned work during
shutdown. Tests use controlled loopback servers; production code does not require public Internet
access.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-network
```
