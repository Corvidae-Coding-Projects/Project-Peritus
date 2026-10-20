# Structural runtime remediation

User scope: repair all six source-review findings; verify actual behavior, commit and push to
develop, then review measured runtime performance. Preserve features, durability and recovery.
Do not generate or run unit suites, formal verification or repository qualification gates.

Baseline: `5f808cd09`, retained as an isolated worktree under
`/home/doll/.cache/peritus-structural-20261020/baseline-source`. Builds run in a dedicated systemd
scope with two Cargo workers, 3 CPU equivalents and a 3200 MiB memory ceiling.

## Repair boundaries and independent expectations

1. Run state: isolate mutation/persistence per run. An intentionally slow persistence operation
   for A must not prevent observing or controlling B. Acknowledgments still require durable state.
2. Control replay: retain verified conversations independently and carry unaffected cache entries
   over exact host-owned commits. Alternating sessions must reuse prior verification; external
   changes must invalidate it. Eviction affects optimization only, never retained history.
3. Startup: keep archival execution payloads cold, hydrate on use, preserve retry, candidate
   reconciliation and corruption isolation. Increasing archival payload size must not require
   proportional resident payload memory or delay readiness for unrelated sessions.
4. Client coupling: move shared presentation/attachment/candidate contracts out of the execution
   coordinator. The TUI must not depend on product-runner. Preserve exact fingerprints and outputs.
5. Plugin failures: continuously drain stderr, keep a bounded tail and exit status, and attach them
   to actual transport failures. A noisy plugin must not fill the pipe or grow diagnostic memory
   without bound. Cancellation still owns and reaps its process tree.
6. Dependencies: put the generic plugin SDK in foundation and its process host in runtime. Keep
   integration adapters above those owners, and reconcile manifests, architecture and waypoints.

## Real operation and performance evidence

Use disposable daemon state/workspaces and native consumers of the production interfaces. Preserve
concrete failures and independent expected results under `docs/testing/reproducers/`; raw logs stay
outside the checkout. Exercise restart, repeated and alternating sessions, concurrent queries,
cancel/retry, corrupt/stale saved state, native plugin failure/noise and cleanup. Compare equivalent
baseline/candidate builds and workloads; report startup time, request latency distribution, peak
RSS, read/write volume where available, and cache hits/cold replays. Keep provider/network latency
separate from measured local work. Do not claim an entire-application or multi-hour soak from a
bounded campaign.
