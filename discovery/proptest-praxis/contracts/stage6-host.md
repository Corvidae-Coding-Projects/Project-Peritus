# Stage 6 host-boundary contract

`stage6::host` owns two synthetic model-based campaigns.

* `Lease` replays use, revocation and observation histories through the public `SecretLease` API. The independent lifecycle table requires exact process, environment, sandbox, execution, secret-version and delivery binding; use is half-open at expiry; wrong bindings do not consume a use; revocation and exhaustion are terminal. Fixed cases include an exact-expiry attempt after a rejected drifted use, exhaustion, and revocation. Metrics distinguish accepted, rejected, binding-drift and exact-expiry attempts.
* `Socket` exercises `bounded_path` over absolute, long, relative, Unicode, traversal-final-component and NUL-containing names. The oracle checks validation, exact preservation when within the requested byte ceiling, deterministic bounded output and rejection when even the fixed private endpoint cannot fit. This property is effect-free.
* The separate `examples/stage6_native_socket.rs` gate prepares an oversized path with the current user's UID, checks the private directory's owner and mode, binds and connects a real Unix socket, exchanges exact bytes, removes the socket, verifies owner cleanup and checks an unrelated sibling canary. It is an actual Unix gate when run; it does not represent a Windows named-pipe gate.

The deliberate host mutation changes the lease expiry comparison from `>=` to `>` in `calibration/stage6-host.patch`. The exact-expiry fixed case and generated histories must fail against the mutation and pass after exact source restoration.

Process output accounting is covered by the separate `stage6::process_output`
model and an actual Linux `ExecutionGateway` fixture. That native fixture runs
disposable children, checks allowed and denied filesystem effects, verifies
the output matrix and descendant cleanup, and preserves a sibling canary.
The observed Linux result is recorded separately from these pure host models;
macOS and Windows still require their own native qualification.
