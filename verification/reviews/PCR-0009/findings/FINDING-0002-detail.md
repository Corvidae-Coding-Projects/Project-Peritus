# FINDING-0002: Environment projection used incompatible case and ordering rules

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

Process-plan environment canonicalization did not match the native backend's case-insensitive ordering/collision contract. A lowercase `peritus_ci_order` entry reproduced an order mismatch, and names that collide after canonical case folding could cross layers with different identities. The repair canonicalizes names through the shared uppercase key, orders the selected set deterministically, and rejects case-folded collisions before launch.
