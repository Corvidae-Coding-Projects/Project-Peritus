# peritus-provider-onboarding

## Folder guide

User-facing provider discovery and official account login for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-provider-onboarding`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Provider discovery, selection, credential references, official runtime installation/login, model discovery, and real connection stages. |

`peritus-provider-onboarding` is the C-class G4 adapter for user-visible provider availability,
account status, and interactive login. It delegates subscription authentication to the official
credential-owning `codex` and `claude` executables and retains no account tokens or command output.

After explicit user approval, it downloads and runs the selected vendor's native installer.
Downloads require HTTPS and have time and size limits. Installation does not sign in or grant
provider authority. Discovery checks PATH and the vendor's standard native directory; callers pass
the resulting canonical executable path to the daemon without changing the process environment.

Windows account-status probes run without an attached console, so provider startup cannot rename
the user's terminal even when standard output is captured. Claude subprocesses also receive its
documented title-update opt-out. Interactive login retains terminal access.

The crate observes only bounded status facts and process exit status. It does not grant provider,
tool, workspace, or approval authority; validated selections are persisted by
`peritus-product-state` and instantiated by G0/C5.

Direct API credentials are captured by the product UI, immediately moved into zeroizing material,
and published through the operating-system credential store. This crate returns only opaque exact
references and non-secret provider settings; it never places API keys in product state or daemon
configuration.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.

The optional ChatGPT direct OAuth route re-exports the C5-owned login and account surface.
It discovers current models through that registration rather than an API-key store. Replacing a
provider selection does not delete or revoke saved OAuth registrations; explicit sign-out owns
revocation. The Codex/Claude executable routes retain their existing ownership.
