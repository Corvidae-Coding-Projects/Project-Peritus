# peritus-local-socket

## Folder guide

Bounded, owner-protected filesystem locations for local Peritus sockets.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-local-socket`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Bounded, owner-protected paths for ordinary Unix-domain sockets. |

`peritus-local-socket` chooses a bounded filesystem location for a local daemon endpoint.
An original path of at most 103 bytes on macOS or 107 bytes on Linux stays unchanged. A longer
path maps to `/tmp/peritus-<32-hex>/daemon.sock`, where the digest binds the complete original
path, including its state root and store-derived endpoint name. Durable state remains in its
configured location.

The daemon holds its existing state-root instance lock before preparing the runtime directory.
The system temporary root must be root-owned and protected against replacement. The private
directory must be a real mode-0700 directory owned by the state-root owner. A pre-existing
symlink, wrong owner, or broader mode is rejected without chmod, replacement, or a fallback.
The daemon protects its socket at mode 0600 and retains its same-user peer authentication.

The daemon, launcher, and platform qualification use the same path derivation. All clients use
ordinary socket APIs; interactive clients retain Tokio's nonblocking, cancellable connect.
After removing its exact socket, the daemon drops the prepared path, which removes an empty
runtime directory only when its device/inode still match. Stale sockets are reclaimed only
under the existing daemon instance lock and ownership checks.

The crate is a class-H adapter. Verus verifies the executable path-length and directory-policy
predicates; filesystem observations, SHA-256, and standard socket behavior remain host boundaries
covered by native tests. It introduces no unsafe production code or proof assumptions.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
