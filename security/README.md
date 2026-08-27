# H0 security qualification assets

This directory is the versioned declarative input to H0. These files describe the threat model,
control/probe traceability, unsafe and trusted-computing-base inventory contracts, and interchange
schemas. They are not proof that a release passed qualification.

Every H0 campaign binds their aggregate digest through the integrated candidate's immutable
qualification-plan digest. Native adapters must reconcile the inventories against the exact source
and artifact digests named by that candidate. The independent review is a separately supplied
artifact and cannot be produced by the qualification crate.

Canonical generated evidence uses `security/schemas/evidence-manifest-v1.schema.json`. Raw process,
terminal, model, and secret-bearing output stays outside the manifest in a controlled artifact store;
the manifest retains only SHA-256 and byte counts.
