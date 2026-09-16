# FINDING-0001 — cleanup controller ownership race

Original severity: high, blocking.

Disposition: fixed.

A hosted macOS testing shard for PR 74's synthetic merge of the earlier `d0e4f0cf` source first exposed the failure when a cleanup controller exited between a response-channel timeout and the subsequent child-status poll. That path completed process-tree and output teardown, consumed the valid response queued during teardown, and then entered the common cleanup tail, which attempted to wait on the already released child and reported `native controller was released`. The precursor candidate `5cbd9efd5ef7719b1505d64350df56e1a0b58892` / `d9a5c4f619f6ba48f8e91cb4335cc49cd6ed6cce` retained the same defective `process.rs` bytes, so it was disqualified before its own hosted qualification completed.

The exact reviewed candidate `65f16e2933b70305de865750931bb05fe9afb64d` / `ae2a43f9a9b73f5fec944d39cf165a8b3ec4df20` retains the successful cleanup `ExitStatus` across response consumption and skips the second wait. It also preserves the output-limit diagnostic if output observed during completed teardown exceeds the stage allowance. The deterministic regression withholds response completion until process-tree teardown, proving that the old bytes fail on the forced ordering and the candidate bytes pass. Focused green, the full `peritus-resilience` suite, strict Clippy, and the exact old/fixed source guard are retained in one length-delimited evidence bundle together with the hosted red trace.

Candidate repair inventory:

- `crates/app/testing/peritus-resilience/src/native/process.rs`: `e0245aa072b0fd8268a4b8637018083794df6f8a08a33d693042ec50823f5727`
- `crates/app/testing/peritus-resilience/src/native/process/output.rs`: `80520f45b40fd014f9eb35422ddb8150afc4be07c01cdebf690e9d336e74d7f1`
- `crates/app/testing/peritus-resilience/tests/native_controller.rs`: `f432167d7002c156a04d5319874496383926de9a21ce6518ac9ced22d24a510e`
- `crates/app/testing/peritus-resilience/tests/native_controller/cleanup.rs`: `dd577e6b4d516e0e7e20b45ca97dcc659ca37743a934a484bfc7a76b1f29e0a8`
