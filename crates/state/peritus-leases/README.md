# peritus-leases

`peritus-leases` is the pure state and observation-contract boundary for Peritus mutation leases.
It binds one workspace lineage to one resolved resource and environment, tracks exact actor/session
ownership, and fences every stale generation before another holder can acquire.

The crate returns logical plans and unprivileged claims only. Its compare-and-swap observations do
not prove persistence, and none of its values is an effect permit. C0 owns durable commit receipts;
the target C1/C2/C4 authorization gateway owns the private permit that can reach an effect.

Authority time is epoch-bound and monotonic. An epoch change or regression is accepted only by an
explicit fencing/reconciliation path. Expiry alone never establishes holder quiescence.
