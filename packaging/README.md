# Peritus native packaging

The host-native package contains the four platform executables required by the product and sandbox
boundary: `peritus`, `peritusd`, `peritus-tui`, and the target's C3 sandbox helper. It also contains
install, upgrade, and uninstall adapters plus the retained native supervisor template.

Run `cargo xtask product-package` at the repository root. The assembler performs a locked,
resource-capped release build and writes `dist/peritus-<platform>-<architecture>` with a canonical
`manifest.toml` and `SHA256SUMS`. `cargo xtask product-install` assembles that package and invokes
the target's per-user installer. Build outputs and assembled packages remain ignored and must never
be committed.

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
