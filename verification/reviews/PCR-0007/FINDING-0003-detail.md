# FINDING-0003 — diagnostics could exceed negotiated frame limits

Original severity: medium, blocking.

Disposition: fixed.

Error diagnostics were bounded by local defaults but were not constrained to the peer's negotiated diagnostic byte limit. A large diagnostic could therefore turn the useful error into a framing failure.

The reviewed candidate applies UTF-8-safe byte truncation before response framing while preserving the error code, retry disposition, and subsystem. The regression negotiates a 24-byte diagnostic limit and proves the encoded response stays valid.
