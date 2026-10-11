# peritus-sandbox-windows

`peritus-sandbox-windows` is the Windows 11 24H2 and Windows Server 2025 native enforcement
backend for Peritus C3. It projects an already checked C2 sandbox plan into a deterministic helper
manifest, an AppContainer or restricted low-integrity token, a kill-on-close Job Object, exact
temporary workspace ACL changes, a closed inherited-handle set, dimension-specific resource
controls, and a supervised teardown record.

The crate never grants execution authority and has no public spawn API. Production preparation is
reachable only through `peritus-process::NativeSandboxBackend`, after the process gateway has
validated and durably consumed the exact action. The helper is the direct C2 child and uses the
fixed ready/manifest/activation protocol before launching the literal target argv.

Unsupported host controls fail closed. Managed egress uses a backend-owned dynamic Windows
Filtering Platform session bound to the target's exact AppContainer package SID. Its highest-weight
filters permit TCP only to the exact IPv4 loopback managed-proxy endpoint and block every other
IPv4/IPv6 outbound connection for that identity. The native session owns the BFE engine handle, so
explicit release or drop removes the nonpersistent sublayer and filters. Probe or installation
fails closed when BFE is unavailable, the caller lacks filter-management rights, the identity is a
restricted token without an exact package SID, or any filter cannot be installed. AppContainer
without network capabilities remains the production deny-all route. Temporary ACL changes are
recorded before modification and restored during release. Device paths, alternate data streams,
reserved names, case-fold aliases, and reparse escapes are rejected during preparation.

Platform-neutral manifest, projection, lifecycle, recovery, and refinement tests run on every
host. Windows-native enforcement tests are ordinary `cfg(windows)` tests and run on a Windows
runner; cross-compilation proves the GNU Windows build path locally but is not a substitute for H2
packaged-host qualification.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-sandbox-windows
```
