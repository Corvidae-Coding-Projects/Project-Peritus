# peritus-local-socket

`peritus-local-socket` binds and connects Unix-domain sockets whose paths are longer than the
standard `sockaddr_un` structure can carry. The standard library refuses such paths before the
kernel sees them: `sun_path` holds 104 bytes on macOS and 108 on Linux, and Peritus keeps its
daemon endpoint beneath a protected per-user state root whose length depends on the platform
layout and the account name.

On macOS the crate passes the kernel an address whose `sun_len` covers the whole path, which XNU
accepts up to 252 bytes. On Linux it addresses the socket through `/proc/self/fd/<directory>/<name>`
so the address stays short regardless of the real path. Paths that fit the standard structure use
the standard library unchanged, and every path is created and validated at its real location.

The daemon, launcher, CLI, TUI, and qualification clients all connect through this crate so that
a socket bound at a long path is reachable from every Peritus process.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-local-socket
```
