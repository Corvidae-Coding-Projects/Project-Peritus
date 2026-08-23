# peritus-codec

`peritus-codec` owns Peritus's canonical binary primitives, versioned frame header, bounded
reader/writer, and SHA-256 helpers. It is domain-neutral: lifecycle, policy, budget, and acceptance
messages are defined by `peritus-protocol`.

The codec treats every input byte as untrusted. It checks lengths before allocation or slicing,
rejects unknown primitive tags and trailing bytes, and never treats a digest as authenticity or
authority evidence.
