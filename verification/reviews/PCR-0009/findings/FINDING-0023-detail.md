# FINDING-0023: Persisted preview evidence lacked complete corruption and retry fences

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The initial persistence path did not bind every launch, command, match, and source fact, and retry could rescan before consulting a previously admitted operation. Corrupt or altered persisted data could therefore be interpreted inconsistently, and retries could depend on changed output availability. The repair adds a domain-separated canonical binding digest, validates command/process/needle/stream/range/source identities on restore, and loads an exact-fingerprint admitted operation before any scan.
