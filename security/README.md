# Security qualification assets

This directory contains the versioned inputs for H0 security qualification: the threat model,
control-to-probe map, unsafe-code and trusted-computing-base inventory rules, and evidence schemas.
They describe what the campaign must check. They do not prove that a release passed.

Every H0 campaign binds their aggregate digest through the integrated candidate's immutable
qualification-plan digest. Native adapters must reconcile the inventories against the exact source
and artifact digests named by that candidate. The independent review is a separately supplied
artifact and cannot be produced by the qualification crate.

Canonical generated evidence uses `security/schemas/evidence-manifest-v1.schema.json`. Raw process,
terminal, model, and secret-bearing output stays outside the manifest in a controlled artifact store;
the manifest retains only SHA-256 and byte counts.

See the [H0 security guide](../docs/h0-security-qualification.md) for the campaign, boundaries, and
release verdict.
