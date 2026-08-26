# peritus-evolution

`peritus-evolution` is the F0 authority for evidence-backed harness evolution. It validates
immutable E1 revisions, published E2 diagnosis, published E3 evaluation, and completed independent
D2 review; builds bounded change manifests and isolated variants; attributes declared predictions;
and applies a deterministic deny-wins promotion policy.

The crate owns two pure aggregate models. An evolution campaign terminates after selection and
promotion review. A project-scoped production pointer outlives campaigns and records append-only
activation and rollback history. Neither reducer performs I/O or grants promotion authority.
Canonical wire frames, C0 persistence/replay, approve-once atomic activation, artifact/evidence
publication, and deterministic recovery remain separate narrow integration boundaries in the same
crate.

All production constructors reject drift and noncanonical or over-limit input. E2, E3, selection,
and review values remain inert evidence: only a later exact B0/B1/C0 authorization gateway may
commit a production activation.

See [`docs/f0-evolution.md`](../../../docs/f0-evolution.md) for aggregate ownership, promotion and
rollback workflows, recovery guidance, protocol families, and serialized verification commands.
