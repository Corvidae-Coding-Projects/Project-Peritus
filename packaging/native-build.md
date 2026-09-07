# Native release compilation

The release workflow builds every product binary twice on its native operating
system and architecture: a primary build and an independent build. It assembles
each role's own binaries and requires complete byte-identical archive/checksum
outputs before release attestation. Manual validation cannot create a release,
sign packages, or publish. Passing a development run is not final H4 qualification.

## Intel macOS daemon phases

The Intel macOS daemon exceeded the ten-minute job ceiling even with four build
jobs. Its library and final binary now have separate bounded native phases.
The other native binary commands, supported platforms, release profile, locked
dependencies, and ten-minute limits are unchanged. The binary matrix waits for
both independent library producers before starting; no role borrows another
role's compilation.

The existing library target is selected with `cargo build --release --locked
--package peritus-daemon --lib`. The final phase still runs the corresponding
`--bin peritusd` build. Cargo's [target selection](https://doc.rust-lang.org/cargo/commands/cargo-build.html#target-selection)
does not change the declared release profile.

The reviewed entry points are:

```sh
export PERITUS_RELEASE_BUILD_ROLE=primary  # independent has a separate fresh producer
cargo xtask release-daemon-library
# In a fresh checkout at the same absolute path and with the same native inputs:
# restore only this role's same-run target/native-daemon-libraries artifact
cargo xtask release-daemon-binary
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
observations, missing libraries, and prebuilt daemon binaries are rejected.
No shared cross-run cache or previously compiled product can satisfy the handoff.
The library and binary invocations retain distinct real times and identifiers.

The final phase retains `target/native-daemon-build.json` alongside its separately
uploaded binary. Assembly retrieves the exact same-run, same-role record and
checks its daemon hash against the actual archive member, not a loose package
projection. Native assembly observation schema v2 retains this compilation chain;
other platforms record no daemon handoff. Old schema-v1 development observations
do not qualify a new candidate. The independent comparison remains mandatory.

## Verification

`cargo test --locked --package xtask` includes the Python transport, source-input,
compile-failure, archived-binary, and native workflow boundary regressions.
Real fresh-source transfer and unsplit-build byte comparison are also required
before a new workflow is considered ready. Hosted native completion must be
observed on the exact commit; local fixtures cannot establish its timing.
