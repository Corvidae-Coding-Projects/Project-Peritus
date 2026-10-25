# Red Team Phase 4 — real-interface campaign

Date: 2026-10-23.

Result: **Phase 4 passed its bounded source-discovery, local-build, installation, compatibility,
failure, cancellation, and recovery campaign on Linux x86-64 GNU.** The exact final candidate was
operated through both human module-store interfaces. The terminal and rendered browser flows each
discovered the pinned Red Team source, started the host-owned build, observed real preparation and
offline compilation, received an installed disabled package, enabled it only on explicit input, and
prepared the module command. The terminal campaign also launched the built executable through the
native module host and received `Red Team — Phase 3 contained jobs`.

This evidence covers the Phase 4 source-delivery boundary. It does not establish autonomous security
discovery, broad security-family coverage, remediation quality, macOS or Windows source builds, or a
complete Red Team campaign. Unit tests and formal verification were not run.

## Candidate identity

- Project Peritus base: `8933e81370a86b02009471eb517c1bc30bdd815f` on `develop`, with the
  Phase 1–4 working change set applied.
- Module Market base: `6b2cca73161b3f0e4ab004f9a124e4e01e84ca89` on `main`, with the
  Phase 1–4 source change set applied.
- Clean disposable catalog commit: `14abf974c1427267b7d8cf7a622647398f2e5812`.
- Canonical source-tree SHA-256:
  `145350c7c6f3c54546c6925d9e415184ac6f63d161e1cdd8794c3890e14880db`.
- Lockfile SHA-256: `6ec951fa0a297c76e8bcd8d2aa1763668dc28e2582ff29cb7b99179455fe5faa`.
- Toolchain and target: `1.97.1-x86_64-unknown-linux-gnu` and
  `x86_64-unknown-linux-gnu`.
- Recipe SHA-256: `4ed48693ff81dec1da0583e8aa8f36c73a810b723bb67d607483f2c073147e7b`.
- Built executable SHA-256:
  `f7d8085a685ae2466bd419ac3efc455f72f14b02c3433db90cd6b01341d83219`.
- Installed package SHA-256:
  `fea1db7063faad66b1a8a0cab10df61ad17c2231f1c22f0dbefceeed46192c74`.
- Final development executables: `peritus`
  `a2cde3a63b737b56ec16c9236d2635e310b15a1f9ccce461ad2eef48b5f06364`, `peritus-tui`
  `6651b90072d6be3e521d29a5cab4880dd66632473b547dca9420e0b3c0fef63d`, `peritus-web`
  `5497b391d695015d0067937206146d51393211968b4838185499cd5bb60f4110`, and `peritusd`
  `e9e00349f588f8f649845c5784b11914c3662914c909ac35bd1de50260cf183a`.

## Source and installation contract

The legacy schema-one `market.toml` remains unchanged. Current clients additionally read
`sources.toml` and atomically retain `catalog-set.json`, while continuing to publish `catalog.json`
for older clients. Refresh only validates and caches catalog metadata. Compilation starts only from
the explicit Build and install action.

The host revalidated a clean exact commit and the canonical source and lockfile digests. It rejected
symlinks, special files, unsupported lockfile origins, mutable checkout state, identity changes, and
undeclared output. Dependency preparation ran without inherited credentials. Compilation ran
offline with network unshared inside a named, finite systemd user scope. The package was rendered in
private state, validated, atomically installed outside Git, and left disabled. Source, archive,
recipe, executable, and package identities remain separate in the durable record.

## Human interface observations

The exact final TUI evidence is retained at `target/phase4-tui-final2/evidence.json` with SHA-256
`df9d01b117ebd559ca13adc7e6b6981d1ea90766e8ca6cdcb073fbc8e87bd717`. The user navigated to
Red Team, pressed `b`, saw `Source build: installed`, pressed `e`, saw the enabled row, pressed
Enter, received `/module peritus.red-team`, and then invoked `status` through the built package.
The retained terminal screen has SHA-256
`817e5541b53af09cd8976159afd29cf5174c9b7afe1476ce93146e8b78b558de`.

The exact final browser evidence is retained at `target/phase4-web-final2/evidence.json` with
SHA-256 `0c353934716c97fa027e8f2667d1cce0ea36109b073b678e7bee24f7e714aea1`.
Real Chromium observed source availability and the preparing-source, preparing-dependencies,
compiling, packaging, and installed phases. It independently observed Installed · Disabled,
clicked Enable, observed Installed · Enabled, clicked Use module, and received the same module
draft. The rendered screenshot has SHA-256
`ea907db607f41ce8c60cd2431eae2fcdd60528de970a4351e430900ae3233b32`.

## Failure, cancellation, and restart observations

- A second client observed the exact live compile owner and named systemd scope without falsely
  converting it to interrupted. It then requested cancellation through the public CLI. Attempt 1
  became `cancelled`, published no identity, stopped the scope, and cleared its exact owner lease.
- Attempt 2 was terminated by sending `TERM` only after matching both the retained PID birth token
  and command line. A fresh process changed it to `interrupted`, stopped the exact retained scope,
  cleared the owner, and did not install partial output.
- Explicit attempt 3 then completed and installed the same package identity. No build scope remained.
- A clean fixture whose lockfile declared an unapproved Git source failed before compilation. The
  previous package digest remained selected and disabled.
- A separate clean fixture containing a deliberate Rust `compile_error!` entered the real bounded
  offline compiler, failed with exit 101, and likewise left the prior package selected.

The compact machine-readable companion is
[red-team-phase4-evidence.json](red-team-phase4-evidence.json). Disposable raw records remain under
`target/phase4-final-recovery/evidence/`; the cancelled, interrupted, and successful-retry records
have SHA-256 values `149d7a691c47185002178625e36065b49c322542807644265da98a50b529f06b`,
`18762f5db8a71a71d79eda480f86a59b1f456551734016dc5b3192859993bf87`, and
`678c6056fdfadef249d8a8e0d9d9a6f75d0e32a078fa1cc332bf9e782cf11ca1`.

## Compatibility and repository hygiene

The current host loaded the existing schema-one Deep Research package at digest
`bc47f1367b75158c1817b78d2aec7026da9a891e8673da82c5c0afb16092ed1a`. The previously installed
Peritus 0.1.10 client, executable SHA-256
`e90120b2c451cf753b3b8b8c20dda0f6195c5b6fc144c373a082b49394dee309`, read the current legacy
catalog and installed selections while ignoring the additive source catalog. Existing ABI-one
packages therefore remain legible; an old client does not advertise a source-only module it cannot
build.

The final disposable catalog checkout was clean after every campaign. No Red Team binary, Rust
artifact, package, cache, or campaign state appeared in the source module or its Git worktree. The
only executable package already present in the market checkout was the existing tracked Deep
Research artifact. There were no remaining `peritus-module-build-*` user scopes.

## Build and analysis record

Focused formatting and offline `cargo check` passed for `peritus-modules`, `peritus-cli`,
`peritus-tui`, `peritus-web`, and `peritus-product-runner`. The Module Market Red Team and SDK passed
offline `cargo check` and strict package Clippy. The WebUI production build passed. Strict host
package Clippy still reaches the repository's pre-existing lint backlog outside the Phase 4 files;
the Phase 4 source-build and TUI module-menu files were separately rerun with warnings denied and
had no remaining findings. That limitation is not represented as a clean workspace-wide Clippy run.
