# peritus-tool-protocol

Version-one bounded, canonical tool descriptors and invocation envelopes. The crate owns no
effects: it validates schemas, calls, progress, artifacts, results, controls, and replay identity.

Every public envelope emits deterministic `PTL1` bytes through a shared family/version framing
encoder. `CanonicalEnvelope::parse` provides bounded, lossless framing round trips; semantic
construction remains owned by each typed envelope so untrusted bytes cannot bypass validation.
