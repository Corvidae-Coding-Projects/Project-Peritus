# Security qualification assets

## Folder guide

Security policy/assessment assets and retained dependency findings; distinguish documents from runtime security enforcement.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [control-catalog-v1.toml](control-catalog-v1.toml) | Configuration/contract data for control catalog v1. Sections: `schema`, `owner`, `status`, `requirements`, `acceptance_criteria`. |
| [tcb-inventory-v1.toml](tcb-inventory-v1.toml) | Configuration/contract data for tcb inventory v1. Sections: `schema`, `owner`, `status`, `component`, `qualification`. |
| [threat-model-v1.toml](threat-model-v1.toml) | Configuration/contract data for threat model v1. Sections: `schema`, `owner`, `status`, `asset`, `threat`. |
| [unsafe-inventory-v1.toml](unsafe-inventory-v1.toml) | Configuration/contract data for unsafe inventory v1. Sections: `schema`, `owner`, `status`, `policy`, `category`. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [schemas/](schemas/AGENTS.md) | Versioned security-control, assessment, and campaign-report JSON contracts. |

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

The native process boundary uses `native-probe-request-v1.schema.json` and
`native-probe-response-v1.schema.json`. Each response must echo the SHA-256 of its exact request and
carry only bounded structured evidence. These schemas define the transport between the H0 runner
and a reviewed platform probe executable; they are not passing probe evidence by themselves.

`native-shard-v1.schema.json` is the durable cross-host boundary. A passing shard retains the exact
candidate, resource limits, receipt provenance, structured evidence, and cleanup accounting for its
canonical platform subset. Aggregation reparses those values into the domain model; failed or
tampered shard documents remain diagnostics and cannot be admitted as ready input.

`native-candidate-v1.schema.json` carries the exact revision tuple, source digest, release-manifest
digest, and qualification-plan digest shared by every host. It is an identity input, not proof that
the named source or artifacts are authentic; release orchestration must produce and authenticate
those bindings separately.

`native-host-facts-v1.schema.json` records the executing platform and architecture, exact candidate
commit and source digest, pinned Rust compiler facts, hosted-runner image facts when available, and
the exact controller file digest and byte count. The preparer creates one host document per native
shard. Its digest is carried by every receipt from that shard.

`final-report-v1.schema.json` is the single-file final aggregation result. It binds the verified
Ready/NotReady status to the candidate source digest, embeds the exact canonical evidence-manifest
JSON, records its SHA-256, and retains stable failure reasons without granting H4 release authority.

See the [H0 security guide](../docs/h0-security-qualification.md) for the campaign, boundaries, and
release verdict.
