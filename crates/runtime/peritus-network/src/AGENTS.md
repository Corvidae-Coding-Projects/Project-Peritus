# crates/runtime/peritus-network/src — navigation

## Responsibility

Connection planning and bounded managed outbound proxy.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting.rs](accounting.rs) | Non-wrapping connection and aggregate network accounting. |
| [cancellation.rs](cancellation.rs) | Cloneable process-local cancellation token. |
| [canonical.rs](canonical.rs) | Versioned canonical network-plan encoding. |
| [credential.rs](credential.rs) | Opaque routing tokens and exact upstream credential leases. |
| [error.rs](error.rs) | Stable network failures and recovery guidance. |
| [lib.rs](lib.rs) | Connection planning and bounded managed outbound proxy. |
| [matcher.rs](matcher.rs) | Deny-dominant requested-name and resolved-address matching. |
| [observation.rs](observation.rs) | Bounded normalized managed-network observations. |
| [plan.rs](plan.rs) | Digest-bound runtime projection of a checked C2 network contract. |
| [preparation.rs](preparation.rs) | Inert managed-proxy configuration consumed during native preparation. |
| [recovery.rs](recovery.rs) | Versioned nonsensitive managed-proxy recovery records. |
| [redirect.rs](redirect.rs) | Bounded redirect-chain revalidation. |
| [refinement.rs](refinement.rs) | Named C3 managed-network refinement obligation. |
| [resolution.rs](resolution.rs) | Per-connection DNS resolution with exact answer revalidation. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `network_decision_no_broader`, `network_charge_allowed`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [proxy/](proxy/AGENTS.md) | Bounded loopback HTTP/CONNECT proxy owner. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
