# peritus-secrets

`peritus-secrets` resolves exact C2 `SecretReference` values through platform credential stores,
retains values only in zeroizing non-clone material, and delivers them through exact expiring
leases. Ordinary debug, error, canonical, observation, recovery, and artifact surfaces contain
references and keyed redaction fingerprints, never secret bytes.

The in-memory store exists only for deterministic tests. Production probes fail closed when the
platform credential service is unavailable.

The platform adapter also provides the product onboarding boundary for writing, replacing, and
removing credentials. Those operations accept zeroizing `SecretMaterial` and return only exact
content-bound `SecretReference` values.
