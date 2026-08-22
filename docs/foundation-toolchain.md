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
xtask package through Cargo's built-in `run` command rather than trusting the alias. A legacy
`rust-toolchain` selector, a symbolic `rust-toolchain.toml`, and any repository Cargo configuration
outside the exact regular root file are rejected. The Just recipe graph and canonical CI jobs are
validated as execution structures, including triggers, runners, step order, shell/failure
semantics, and the full advisory/license/source gate—not merely searched for command text.

The checked `.github/workflows/formal-governance.yml` emits the stable `Gate A` status required by
the GitHub Team repository ruleset. Candidate-code-executing Rust operations remain isolated into
clean matrix jobs. A final always-running aggregation job fails unless policy, workflow lint, every
Rust matrix entry, supply-chain policy, and Verus/no-cheating verification and builds all succeed.
Every checkout is explicit and credential-free, every Cargo-bearing job verifies the reviewed
Cargo-configuration digest first, and workflow lint uses a separately digest-checked actionlint
archive.

The repository locks both the complete workflow and the exact repository-ruleset payload template,
but cannot attest to live GitHub state. Gate A therefore also requires the activation and API
evidence in [GitHub governance](github-governance.md). GitHub Team cannot pin a required workflow to
an immutable reviewed revision, so the current status-check system remains candidate-controlled;
the stronger Enterprise authority is an explicit future upgrade rather than a current claim.

## Compilation-input trust discovery

A0 seeds trust scanning from every workspace Cargo target, follows direct literal `include!` and
`#[path]` inputs regardless of file extension, and rejects dynamic, ignored, external, generated
without controlled ownership, or symbolic-link inputs. The Rust token `include` is reserved for
direct literal `include!` declarations. Repository `macro_rules!` definitions and workspace
procedural-macro targets are rejected because lexical trust discovery cannot soundly enumerate
macro-synthesized compilation inputs. External executable preprocessing is separately
fail-closed. Full locked Cargo metadata is searched for every dependency build-script and
procedural-macro target; only these exact package identities are admitted:

- build scripts: `libc@0.2.189`, `proc-macro2@1.0.107`, `quote@1.0.47`, `serde@1.0.229`,
  `serde_core@1.0.229`, `serde_json@1.0.149`, `zmij@1.0.23`, and the pinned Verus revision's
  `verus_prettyplease@0.0.0-2026-08-09-0044`, `verus_syn@0.0.0-2026-08-02-0125`, and
  `vstd@0.0.0-2026-08-09-0044`;
- procedural macros: `serde_derive@1.0.229` and the pinned Verus revision's
  `verus_builtin_macros@0.0.0-2026-08-09-0044` and
  `verus_state_machines_macros@0.0.0-2026-08-02-0125`.

The executable identity includes registry or immutable Git source, package name, and exact
version—not just the readable labels above. Adding or changing one requires source review,
lockfile and allowlist updates, proof-impact review where formal semantics change, and a clean
Gate A run. A1 then adds semantic TCB manifests and trust-occurrence reconciliation rather than
claiming expansion-complete lexical analysis.

A1 formal packages additionally reject `env!`, `include_str!`, and `include_bytes!`. Those macros
can inject environment or arbitrary data into executable and specification semantics without
creating a Rust compilation-source edge. Prohibiting them keeps every admitted formal input inside
the exact source, package-manifest, workspace, toolchain, lockfile, and governance fingerprints.
