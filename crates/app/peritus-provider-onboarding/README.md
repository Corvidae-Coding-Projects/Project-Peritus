# peritus-provider-onboarding

`peritus-provider-onboarding` is the C-class G4 adapter for user-visible provider availability,
account status, and interactive login. It delegates subscription authentication to the official
credential-owning `codex` and `claude` executables and retains no account tokens or command output.

After explicit user approval, it downloads and runs the selected vendor's native installer.
Downloads require HTTPS and have time and size limits. Installation does not sign in or grant
provider authority. Discovery checks PATH and the vendor's standard native directory; callers pass
the resulting canonical executable path to the daemon without changing the process environment.

The crate observes only bounded status facts and process exit status. It does not grant provider,
tool, workspace, or approval authority; validated selections are persisted by
`peritus-product-state` and instantiated by G0/C5.

Direct API credentials are captured by the product UI, immediately moved into zeroizing material,
and published through the operating-system credential store. This crate returns only opaque exact
references and non-secret provider settings; it never places API keys in product state or daemon
configuration.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-provider-onboarding
```
