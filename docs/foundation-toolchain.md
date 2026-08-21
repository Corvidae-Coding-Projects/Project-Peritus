# Foundation toolchain policy

A0 pins Rust `1.97.1`, Verus `0.2026.08.09.92f466f`, vstd commit
`92f466f247f45128c630d1c843fd6e27d2115587`, and the Z3 `4.16.0` executable shipped in the
digest-checked Verus archive. `toolchains.toml` is the canonical machine-readable record;
`cargo xtask reproducibility-check` rejects drift in the Rust manifest, workspace metadata, vstd
Git revision, archive URL/digest, lockfile, canonical recipes, and canonical CI jobs. Extra CI
actions must still use immutable revisions; changing a reviewed immutable reference remains an
ordinary code-review decision rather than an automatically forbidden update.

## Solver metadata discrepancy

The pinned `cargo-verus` currently advertises Z3 `4.12.5` from its generated toolchain metadata,
while the same Verus revision's verifier source requires Z3 `4.16.0` and its official archive
actually bundles Z3 `4.16.0`. Peritus records both facts instead of weakening verification:

- `z3 = "4.16.0"` is the executable pin and the value `xtask toolchain-check` executes and
  validates.
- `cargo_verus_advertised_z3 = "4.12.5"` is a checked observation that makes an upstream metadata
  change visible.
- CI never disables the solver version check. A solver-version bypass in the `justfile` or CI is a
  reproducibility-policy violation.

## Accepted Verus cfg names

Workspace lint policy denies unknown cfg names. The allowlist is deliberately limited to
`verus_only`, used by the documented cargo-verus workspace integration, plus `verus_keep_ghost`,
`verus_keep_ghost_body`, and `verus_verify_core`, which are present in the pinned Verus/vstd
manifests and compiler sources. Adding another cfg requires toolchain evidence and review; ordinary
application-specific cfg names are not globally exempted.

## Dependency policy

Direct registry dependencies use exact `=version` requirements. Git dependencies use immutable
40-character revisions. `Cargo.lock` is committed, and every dependency-resolving developer and CI
command uses `--locked`. GitHub Actions use immutable commit SHAs. `cargo-deny` denies unknown
registries, unknown Git sources, wildcard requirements, duplicate versions, unapproved licenses,
and known advisories under the checked-in policy.

Path dependencies are accepted only when they resolve without symbolic links to registered
workspace packages inside this repository. External and unregistered in-repository path crates are
mutable inputs that a Cargo lockfile cannot freeze, so A0 rejects them.

## Checked command execution

The root `.cargo/config.toml` is the only repository Cargo configuration. Its complete schema is
checked, including the locked `cargo xtask` alias; legacy and nested Cargo configurations are
rejected so member-directory invocation cannot silently replace a gate. A non-Cargo bootstrap job
checks the reviewed configuration digest before any Cargo job runs, and gate recipes invoke the
xtask package through Cargo's built-in `run` command rather than trusting the alias. The Just recipe
graph and canonical CI jobs are validated as execution structures, including triggers, runners,
step order, shell/failure semantics, and the full advisory/license/source gate—not merely searched
for command text. Authority to approve a simultaneous workflow-and-digest change remains an
external repository ruleset/review responsibility; an in-repository checker cannot grant itself
that authority.

## Compilation-input trust discovery

A0 seeds trust scanning from every workspace Cargo target, follows direct literal `include!` and
`#[path]` inputs regardless of file extension, and rejects dynamic, ignored, external, generated
without controlled ownership, or symbolic-link inputs. The Rust token `include` is reserved for
direct literal `include!` declarations. Repository `macro_rules!` definitions and workspace
procedural-macro targets are rejected because lexical trust discovery cannot soundly enumerate
macro-synthesized compilation inputs. External procedural macros remain controlled by exact pinned
dependencies and full verification; A1 adds the semantic TCB manifest and trust-occurrence
reconciliation rather than claiming expansion-complete lexical analysis.
