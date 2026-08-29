# Peritus native packaging

The host-native package contains the four platform executables required by the product and sandbox
boundary: `peritus`, `peritusd`, `peritus-tui`, and the target's C3 sandbox helper. It also contains
install, upgrade, and uninstall adapters plus the retained native supervisor template.

Run `cargo xtask product-package` at the repository root. The assembler performs a locked,
resource-capped release build and writes `dist/peritus-<platform>-<architecture>` with a canonical
`manifest.toml` and `SHA256SUMS`. `cargo xtask product-install` assembles that package and invokes
the target's per-user installer. Build outputs and assembled packages remain ignored and must never
be committed.

After the first public release, users will install through one command:

```sh
curl -fsSL https://raw.githubusercontent.com/Corvidae-Coding-Projects/Project-Peritus/main/install.sh | sh
```

Windows PowerShell uses:

```powershell
irm https://raw.githubusercontent.com/Corvidae-Coding-Projects/Project-Peritus/main/install.ps1 | iex
```

Both bootstraps resolve the latest GitHub release, select the host archive, verify its published
SHA-256 digest, and call the same native install or upgrade adapter described below. `peritus`
checks for updates at most once every six hours without blocking offline startup; `peritus update`
runs the check immediately. `peritus update --disable-checks` persists an opt-out, and
`peritus update --enable-checks` turns automatic checks back on.

Installation is deliberately per-user because G0 authenticates local peers as the protected state
root owner. The installer verifies all package checksums, atomically publishes the launcher,
daemon, TUI, helper, and optional supervisor template, and makes `peritus` the ordinary command.
It does not ask the user to construct daemon configuration or register a background service.
The launcher owns first-run identities, generated configuration, provider/workspace onboarding,
and bounded daemon startup/reuse.

Upgrade snapshots only package-owned files, invokes the same checked installer, and restores the
snapshot if publication fails. Ordinary uninstall removes package files and any legacy supervisor
registration while preserving configuration generations, provider credentials, durable state,
managed worktrees, logs, and diagnostics. Destructive purge of those protected roots is outside
these assets.

The service templates are retained for a future explicitly configured always-on runner mode. They
are not installed as active services and are not part of the ordinary single-command product path.

`cargo xtask release-bootstrap-smoke` qualifies the native lifecycle plus the public file-download
path and proves that a mismatched archive checksum is rejected. Tagged release automation keeps the
GitHub release as a draft until the Linux, macOS, and Windows policy, lifecycle, packaging, and
upload jobs all succeed.

There is no public release yet. Until the exact candidate completes production qualification, use
`cargo xtask product-install` from a source checkout.
