# peritus-product-runner

Daemon-owned writer, reviewer, fixer, and repository-gate coordination for the interactive Peritus
product. The crate consumes provider and managed-workspace capabilities resolved by the daemon and
emits bounded progress observations; it does not own UI or authority decisions.
