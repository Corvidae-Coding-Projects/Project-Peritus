# peritus-launcher

## Folder guide

Single-command local bootstrap and product composition for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-launcher`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Single-command bootstrap: product state, workspace trust/selection, provider setup, packaged daemon resolution/startup, update handling, and UI handoff. |

`peritus-launcher` is the C-class G4 host-composition boundary behind interactive `peritus` startup.
It discovers and protects platform-local roots, atomically publishes immutable product-state
generations and generated G0 inputs, resolves the matching packaged daemon, establishes bounded
readiness, and hands the endpoint to `peritus-tui`. Interactive startup allows up to five
minutes for cold recovery or a one-time state migration and prints a progress notice every five
seconds. The daemon log records timings for each completed recovery phase.

Remembered in-place folders retain their workspace/conversation identity after Git initialization
or filesystem device renumbering. Folder identity uses directory birth time and inode where
available; matching legacy trust upgrades automatically. If continuity cannot be established,
Peritus asks to trust the current directory again and retains its conversation lineage. Declining
keeps it restricted. `peritus workspaces` can restore trust later.

Its setup composition presents the complete built-in provider catalog, persists choices before
daemon startup, generates immutable C5 route configuration, and provides focused provider repair
and settings flows. The ordinary path requires neither environment variables nor hand-written
configuration. With two or more selected routes it offers a plain-language automatic-failover
choice, preserves that choice during ordinary repair, and writes it into the immutable daemon
configuration; older and single-provider state remain failover-disabled.

Known generated-format upgrades advance the product-state generation before publishing new
configuration. This includes hosted model-limit discovery, older OpenCode capability lists,
and the retired provider timeout, including combinations from skipped releases. Earlier files,
provider/model choices, credential references, identities, and workspace trust stay intact.
Unrecognized configuration differences still fail without overwriting the existing file.
An interrupted publication resumes from the committed generation on the next launch.

First-run setup and `peritus providers` also offer a review provider/model choice.
Choose `0` to follow the ordinary default, or select an enabled provider and a
discovered or explicitly entered model for independent review. The saved choice
initializes new terminal conversations without changing the writer or fixer.
Existing conversations retain their own durable model choices. Provider repair
keeps the reviewer preference unless its provider is removed; offline mode clears it.

When a selected account provider is missing, setup offers its official installer with consent
defaulting to no. A declined or failed installation never enables that provider. Successful
installation returns to the ordinary login flow, and generated daemon configuration pins the
discovered executable even when the current terminal has an older PATH.
Resolved account executables are persisted before configuration rendering. An official-client
update advances the immutable generation while preserving the selected model and prior files;
temporary discovery failure retains an existing pin so provider repair remains reachable.

The launcher owns host effects but no durable domain authority. It does not interpret A3 commands,
provider traffic, workspace mutations, approvals, or TUI presentation state. All generated
configuration is parsed through the production G0 validator before use, and existing identity-bound
configuration is preserved only when its protected core matches the installation.

## Multiple clients and live workspaces

Additional terminal windows share the daemon. New direct folders and managed Git registrations
are admitted through immutable workspace catalog generations, with roots, folder trust, and host
permissions replaced together. Existing work continues; selecting a read-only window does not
change installation-wide tool availability or grant it write authority.

A launch lock serializes concurrent daemon startup/publication. Removing a folder from recents
retains its admitted binding until restart, so another window can keep using it. Temporarily
unavailable inactive folders do not prevent opening a different folder. Provider/executable
changes and replacement of existing workspace identities require a planned graceful restart.
A new window preserves the running daemon and gives an actionable error when a live update is
unsafe or the older daemon does not implement it. Normal workspace additions do not restart
the service.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
