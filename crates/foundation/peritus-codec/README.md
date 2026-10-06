# peritus-codec

`peritus-codec` owns Peritus's canonical binary primitives, versioned frame header, checked
reader/writer, incremental frame receipt, and SHA-256 helpers. It is domain-neutral: lifecycle,
policy, budget, and acceptance messages are defined by `peritus-protocol`.

The codec treats every input byte as untrusted. It checks lengths before allocation or slicing,
rejects unknown primitive tags and trailing bytes, and never treats a digest as authenticity or
authority evidence.

The production/default contract has no logical byte, collection, or nesting quota. Version-one
`u32` prefixes remain representation boundaries; existing canonical bytes and digests are unchanged.
The former production profile is frozen as `CodecLimits::LEGACY_V1` for compatibility. Explicit
caller and negotiated peer contracts still apply symmetrically to encoding and decoding.

Collection decoders provide their schema's minimum encoded item width before reserving owned
storage. Impossible counts reject against the received bytes, and allocation failure has its own
typed capacity error. `FrameReceiver` validates the full header before payload receipt and reads
through repeatable physical windows, retaining partial bytes across cancellation and retry. Its
window size bounds an input operation, without limiting the number of windows in a frame.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-codec
```
