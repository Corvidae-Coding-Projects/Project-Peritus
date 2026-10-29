# crates/runtime/peritus-secrets/src — navigation

## Responsibility

Exact secret leases, zeroizing material, scoped delivery, and redaction support.

## Files in this folder

| File | Role |
| --- | --- |
| [delivery.rs](delivery.rs) | Exact environment, private-file, and brokered-handle delivery staging. |
| [error.rs](error.rs) | Stable secret failures with non-content-bearing diagnostics. |
| [fingerprint.rs](fingerprint.rs) | Keyed domain-separated redaction fingerprints. |
| [lease.rs](lease.rs) | Exact expiring one-owner secret delivery leases. |
| [lib.rs](lib.rs) | Exact secret leases, zeroizing material, scoped delivery, and redaction support. |
| [material.rs](material.rs) | Non-clone zeroizing secret material. |
| [preparation.rs](preparation.rs) | Authorized-input preparation of exact secret delivery sessions. |
| [recovery.rs](recovery.rs) | Nonsensitive versioned secret-delivery recovery records. |
| [reference.rs](reference.rs) | Stable opaque text form for exact platform credential references. |
| [refinement.rs](refinement.rs) | Named C3 exact secret-delivery refinement obligation. |
| [store.rs](store.rs) | Exact secret-store boundary and platform credential adapter. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `secret_delivery_exact`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [store/](store/AGENTS.md) | Implementation pieces for `store.rs`: Exact secret-store boundary and platform credential adapter. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
