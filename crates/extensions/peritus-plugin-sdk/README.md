# peritus-plugin-sdk

Stable, serde-based contracts for Peritus plugins that execute outside the daemon address space.
The crate validates canonical manifests, bounded JSON payloads, protocol version negotiation, and
length-delimited request/result frames. Decoding a frame never grants authority; the plugin host
must bind every invocation to current daemon policy.
