# peritus-evolution

## Folder guide

Evidence-backed production harness evolution and promotion policy for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-evolution`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Evidence-backed production harness evolution for Peritus. |

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

Evolution publication directives carry the exact production `RevisionTuple`. The publisher derives
the complete artifact dependency set from the producing C0 journal batch, verifies the directive's
artifact is present, and admits evidence only with those committed bindings.

The optional `qualification` feature supplies deterministic, store-bound fixtures for the real F0
promotion crash boundary. It is used by the daemon's administrative H1 surface and is not part of
ordinary evolution planning.

All production constructors reject drift and noncanonical or over-limit input. E2, E3, selection,
and review values remain inert evidence: only a later exact B0/B1/C0 authorization gateway may
commit a production activation.

See [`docs/f0-evolution.md`](../../../docs/f0-evolution.md) for aggregate ownership, promotion and
rollback workflows, recovery guidance, protocol families, and serialized verification commands.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
