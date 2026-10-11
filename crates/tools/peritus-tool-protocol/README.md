# peritus-tool-protocol

Version-one bounded, canonical tool descriptors and invocation envelopes. The crate owns no
effects: it validates schemas, calls, progress, artifacts, results, controls, and replay identity.

Every public envelope emits deterministic `PTL1` bytes through a shared family/version framing
encoder. `CanonicalEnvelope::parse` provides bounded, lossless framing round trips; semantic
construction remains owned by each typed envelope so untrusted bytes cannot bypass validation.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tool-protocol
```
