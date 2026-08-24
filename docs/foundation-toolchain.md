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
and known advisories under the checked-in policy. The reviewed license set is Apache-2.0,
BSD-3-Clause, ISC, MIT, MIT-0, Unicode-3.0, and Zlib; ISC and MIT-0 cover the pinned
Rustls/AWS-LC cryptography closure introduced by C5 rather than a package-specific exception.

Path dependencies are accepted only when they resolve without symbolic links to registered
workspace packages inside this repository. External and unregistered in-repository path crates are
mutable inputs that a Cargo lockfile cannot freeze, so A0 rejects them.

## Checked command execution

The root `.cargo/config.toml` is the only repository Cargo configuration. Its complete schema is
checked, including the locked `cargo xtask` alias; legacy and nested Cargo configurations are
rejected so member-directory invocation cannot silently replace a gate. A non-Cargo bootstrap job
uses Git to compare both the Cargo configuration and `.gitattributes` against signed authority
commit `6ca5f56d2ab12e93f155d684b33f4a86c2f877b8` before any Cargo job runs, and gate recipes invoke
the xtask package through Cargo's built-in `run` command rather than trusting the alias. A legacy
`rust-toolchain` selector, a symbolic `rust-toolchain.toml`, and any repository Cargo configuration
outside the exact regular root file are rejected. The Just recipe graph and canonical CI jobs are
validated as execution structures, including triggers, runners, step order, shell/failure
semantics, and the full advisory/license/source gate—not merely searched for command text.

The checked `.github/workflows/formal-governance.yml` emits the stable `Gate A` status required by
the GitHub Team repository ruleset. Candidate-code-executing Rust operations remain isolated into
clean matrix jobs. A final always-running aggregation job fails unless policy, workflow lint, every
Rust matrix entry, supply-chain policy, and Verus/no-cheating verification and builds all succeed.
Every checkout is explicit and credential-free, every Cargo-bearing job verifies the signed
pre-Cargo authority first, and workflow lint uses a separately digest-checked actionlint archive.

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

- build scripts: `anyhow@1.0.104`, `async-io@2.6.0`, `aws-lc-rs@1.18.0`,
  `aws-lc-sys@0.44.0`, `crossbeam-utils@0.8.22`, `curve25519-dalek@5.0.0`,
  `generic-array@0.14.7`, `getrandom@0.4.3`, `httparse@1.10.1`,
  `icu_normalizer_data@2.3.0`, `icu_properties_data@2.3.0`, `jni@0.22.4`,
  `jni-macros@0.22.4`, `libc@0.2.189`, `libsqlite3-sys@0.38.2`, `memoffset@0.9.1`,
  `nix@0.28.0`, `nix@0.31.3`, `num-traits@0.2.19`, `proc-macro2@1.0.107`,
  `quote@1.0.47`, `quinn@0.11.11`, `quinn-udp@0.5.15`, `ring@0.17.14`,
  `rustix@1.1.4`, `rustls@0.23.43`, `rustversion@1.0.23`, `serde@1.0.229`,
  `serde_core@1.0.229`, `serde_json@1.0.149`, `thiserror@1.0.69`,
  `thiserror@2.0.20`, `wasm-bindgen@0.2.127`, `wasm-bindgen-shared@0.2.127`,
  `winapi@0.3.9`, `winapi-i686-pc-windows-gnu@0.4.0`,
  `winapi-x86_64-pc-windows-gnu@0.4.0`, `winreg@0.10.1`, `zmij@1.0.23`, all eight
  `windows_*@0.52.6` architecture archives selected by Cargo metadata, and the pinned Verus
  revision's `verus_prettyplease@0.0.0-2026-08-09-0044`,
  `verus_syn@0.0.0-2026-08-02-0125`, and `vstd@0.0.0-2026-08-09-0044`;
- procedural macros: `async-recursion@1.1.1`, `async-trait@0.1.92`,
  `curve25519-dalek-derive@0.1.1`, `displaydoc@0.2.7`, `enumflags2_derive@0.7.12`,
  `futures-macro@0.3.34`, `jni-macros@0.22.4`, `jni-sys-macros@0.4.1`,
  `rustversion@1.0.23`, `serde_derive@1.0.229`, `serde_repr@0.1.21`,
  `thiserror-impl@1.0.69`, `thiserror-impl@2.0.20`, `tokio-macros@2.7.2`,
  `tracing-attributes@0.1.31`, `wasm-bindgen-macro@0.2.127`, `zbus_macros@5.19.0`,
  `zvariant_derive@5.15.0`, `windows-implement@0.60.2`, `windows-interface@0.59.3`,
  `yoke-derive@0.8.2`, `zerofrom-derive@0.1.7`, `zerovec-derive@0.11.6`, and the pinned
  Verus revision's `verus_builtin_macros@0.0.0-2026-08-09-0044` and
  `verus_state_machines_macros@0.0.0-2026-08-02-0125`.

For C2, the reviewed `nix` build scripts use `cfg_aliases` to emit target-derived configuration;
the `winapi`, GNU architecture support, and `winreg` build scripts emit target and feature link
configuration. The reviewed `anyhow` and `thiserror` scripts compile packaged capability probes
with Cargo's selected compiler into `OUT_DIR`, inspect the result, and remove their temporary
probe artifacts; `anyhow` also queries the compiler version. The `thiserror-impl`,
`windows-implement`, and `windows-interface` procedural macros transform caller token streams and
perform no independent network, repository, or child-process access. Their complete executable
identities and dependency closures remain exact-lockfile inputs to Gate A.

For C5, the reviewed HTTP/TLS dependency scripts are platform/compiler configuration and packaged
native-build steps. `httparse`, `rustls`, `quinn`, `quinn-udp`, the ICU data crates, JNI crates, and
Windows architecture archives emit target configuration or link paths. `aws-lc-sys`, `aws-lc-rs`,
and `ring` compile their packaged C/assembly sources with Cargo-selected build tools; they do not
fetch source at build time. The newly admitted procedural macros implement Tokio async
entry/select expansion, display text derivation, JNI bindings, and ICU zero-copy derives from
caller token streams. Their exact versions, registry identities, archive checksums, build inputs,
and complete transitive closures are locked; changing any executable package identity reopens this
review and fails A0 before candidate execution.

For B1, the reviewed `curve25519-dalek` build script reads exactly
`CARGO_CFG_TARGET_FEATURE`, `CARGO_CFG_TARGET_ARCH`, `CARGO_CFG_CURVE25519_DALEK_BITS`,
`CARGO_CFG_CURVE25519_DALEK_BACKEND`, and `CARGO_CFG_TARGET_POINTER_WIDTH`. Those values select the
field width, serial/SIMD/AVX-512 backend, and emitted Cargo configuration. Its build dependency
`rustc_version@0.4.1` reads `RUSTC` and optional nonempty `RUSTC_WRAPPER`, executes the selected
compiler as `<rustc> -vV` (through the wrapper when set), and parses the result using
`semver@1.0.28`; the compiler version and channel can also change the emitted configuration. This
is build-time execution and environment/process input, not a pure target-configuration lookup.

The complete active build-program dependency closure is `curve25519-dalek build.rs ->
rustc_version -> semver`. The complete active derive executable closure is
`curve25519-dalek-derive -> proc-macro2, quote, syn -> unicode-ident`; `proc-macro2` and `quote`
also own separately allowlisted build-script targets. `rustc_version`, `semver`, `syn`, and
`unicode-ident` contribute executable library code to those build or procedural-macro processes
but do not themselves own an active build-script or procedural-macro target. This distinction is
why Cargo metadata target kinds drive the executable-package allowlist while `Cargo.lock` pins the
entire transitive library closure.

The reviewed derive crate has one compile-time `include_str!("../README.md")` used only to form its
own crate documentation. Macro invocation parses caller-provided attribute/item token streams and
emits target-feature wrappers and specializations; that invocation performs no additional file,
network, process, or environment access. The exact registry package IDs and archive checksums for
the executable packages and their closure remain pinned by `Cargo.lock`; a version, registry,
Git/path source, checksum, target-kind, or closure change requires a new proof-impact review.

The executable identity is Cargo's complete package ID: registry or immutable Git source, package
name, and exact version—not just the readable labels above. A same-name/same-version package from
another registry, Git revision, or path is a different identity and fails closed. Adding or
changing one requires source review, lockfile and allowlist updates, proof-impact review where
formal semantics change, and a clean Gate A run. A1 then adds semantic TCB manifests and
trust-occurrence reconciliation rather than claiming expansion-complete lexical analysis.

A1 formal packages additionally reject `env!`, `include_str!`, and `include_bytes!`. Those macros
can inject environment or arbitrary data into executable and specification semantics without
creating a Rust compilation-source edge. Prohibiting them keeps every admitted formal input inside
the exact source, package-manifest, workspace, toolchain, lockfile, and governance fingerprints.
