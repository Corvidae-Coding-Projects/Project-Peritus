# peritus-launcher

`peritus-launcher` is the C-class G4 host-composition boundary behind interactive `peritus` startup.
It discovers and protects platform-local roots, atomically publishes immutable product-state
generations and generated G0 inputs, resolves the matching packaged daemon, establishes bounded
readiness, and hands the endpoint to `peritus-tui`.

Its setup composition presents the complete built-in provider catalog, persists choices before
daemon startup, generates immutable C5 route configuration, and provides focused provider repair
and settings flows. The ordinary path requires neither environment variables nor hand-written
configuration.

The launcher owns host effects but no durable domain authority. It does not interpret A3 commands,
provider traffic, workspace mutations, approvals, or TUI presentation state. All generated
configuration is parsed through the production G0 validator before use, and existing identity-bound
configuration is preserved only when its protected core matches the installation.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-launcher
```
