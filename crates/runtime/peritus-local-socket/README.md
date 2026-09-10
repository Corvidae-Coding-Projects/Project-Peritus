# peritus-local-socket

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

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-local-socket
```
