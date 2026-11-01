# Stage 5 C4 tool contract and oracle

The public target `tools` builds JSON syntax trees with duplicate-preserving object entries and
uses an independent JSON/schema interpreter for constructor validity and semantic validation.
It rejects escaped duplicate keys, noninteger numeric forms, overbound values, missing required
properties, and malformed input. Structural schema compatibility is computed from a separate
normalized representation, including root enumeration replacement and optional object additions.

The envelope oracle assembles descriptor, call v1/v2, prepared call, replay identity, progress,
control, artifact, failure, and result bytes from explicit fields and independent SHA-256
preimages. It checks status, all closed control/progress/failure/truncation tags, exact capacity
edges, identity/provenance binding, and invalid call ceilings. An artifact with another action or
prepared digest must be rejected. Constructive examples reach the closed variants; generated
cases vary bounds and invalid metadata.

Native source owners: `peritus-tool-protocol/src/{json,schema,descriptor,call,prepared,control,
progress,artifact,result,wire}.rs`. These are protocol checks, not authorization of a real host
effect. Stage 6 owns filesystem/process/authority enforcement. The `Header` oracle accepts
nonzero extension families in v1 by contract; only the known C4 family has a v2 form.
