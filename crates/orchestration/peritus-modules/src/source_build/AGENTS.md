# source_build — navigation

Host-owned Linux source building after an explicit module-market action.

| File | Role |
| --- | --- |
| [source.rs](source.rs) | Revalidate/capture the exact clean revision and bind its tree and lockfile digests. |
| [package.rs](package.rs) | Resolve the pinned toolchain, prepare locked dependencies, compile offline and install disabled. |
| [process.rs](process.rs) | Bounded systemd user scopes, Bubblewrap process execution, cancellation and retained logs. |
| [ownership.rs](ownership.rs) | Durable cross-process lease, exact Linux process birth identity and cancellation request. |
| [wait.rs](wait.rs) | CLI terminal wait that retains the owner process until lease and watcher cleanup finish. |

[Parent map](../AGENTS.md). Preparation may use public network access without credentials. Compilation
is network-denied. Only a completed, identity-checked package crosses into the immutable registry.
