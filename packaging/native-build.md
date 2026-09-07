# Native release compilation

The release workflow builds every product binary twice on its native operating
system and architecture: a primary build and an independent build. It assembles
each role's own binaries and requires complete byte-identical archive/checksum
outputs before release attestation. Manual validation cannot create a release,
sign packages, or publish. Passing a development run is not final H4 qualification.

## Intel macOS library and binary phases

The Intel macOS daemon and CLI exceeded the ten-minute job ceiling even with
four build jobs. The daemon library producer and each final binary now have
separate bounded native phases. The other native binary commands, supported platforms, release profile, locked
dependencies, and ten-minute limits are unchanged. The binary matrix waits for
both independent library producers before starting; no role borrows another
role's compilation.

The existing library target is selected with `cargo build --release --locked
--package peritus-daemon --lib`. The final phase still runs the corresponding
`--package peritus-daemon --bin peritusd` build for the daemon, or
`--package peritus-cli --bin peritus` for the CLI. The CLI retains its own
dependency features: Cargo recompiles feature-affected libraries when necessary.
This is partial library reuse, not a forced shared feature graph or a promise
that only the final binary target compiles. Cargo's [target selection](https://doc.rust-lang.org/cargo/commands/cargo-build.html#target-selection)
does not change the declared release profile.

The reviewed entry points are:

```sh
export PERITUS_RELEASE_BUILD_ROLE=primary  # independent has a separate fresh producer
cargo xtask release-daemon-library
# In a fresh checkout at the same absolute path and with the same native inputs:
# restore only this role's same-run target/native-daemon-libraries artifact
cargo xtask release-daemon-binary
# In a separate fresh checkout at that same absolute path, restore the original
# library-only artifact again, never the daemon binary consumer's target tree:
cargo xtask release-cli-binary
```

These commands reject existing compilation/product outputs. They use the fixed
`target/native-daemon` Cargo directory. Linux can exercise the transfer mechanics
locally, but Linux timing is not evidence of native macOS capacity. Windows does
not use this handoff.

## Admission and independence

The library transport contains only `libraries.tar.gz` and `libraries.json`.
It binds the actual complete source inventory, commit, source epoch, version,
release profile, native OS/image, Rust/Cargo/C compiler identities, macOS SDK,
checkout/Cargo paths, capacity, build role, and exact workflow run/attempt.
Changed inputs are rejected before Cargo. Profile, cross-target, wrapper, and
Rust flag or compiler overrides are not accepted; the Rust sysroot is also bound.
Recorded commands must select the exact library or binary phase. Source bytes are reverified before their
timestamps are set to the committed epoch on both fresh checkouts; target
permissions and timestamps are preserved for Cargo's own freshness checks.

The archive is bounded to 2 GiB compressed, 8 GiB expanded, and 100,000 entries.
Links, special files, traversal, duplicate paths, special permissions, malformed
observations, missing libraries, and prebuilt CLI or daemon binaries are rejected.
No shared cross-run cache or previously compiled product can satisfy the handoff.
The library and binary invocations retain distinct real times and identifiers.

Each final phase retains `target/native-peritusd-build.json` or
`target/native-peritus-build.json` alongside its separately uploaded binary.
Assembly requires exactly these two same-run, same-role records, bound to their
respective package and binary, with distinct final invocations and an identical
original library record. It checks both hashes against the actual archive
members, not loose package projections. Binary compilation records use schema v2;
native assembly schema v3 retains both in `binary_compilations`. Other platforms
retain an empty map. Old daemon-only binary/assembly development observations
do not qualify a new candidate under this protocol. The daemon library transport
remains schema v1. The independent comparison remains mandatory.

## Verification

`cargo test --locked --package xtask` includes the Python transport, source-input,
compile-failure, archived-binary, and native workflow boundary regressions.
Real fresh-source transfer and unsplit-build byte comparison are also required
before a new workflow is considered ready. Hosted native completion must be
observed on the exact commit; local fixtures cannot establish its timing.
