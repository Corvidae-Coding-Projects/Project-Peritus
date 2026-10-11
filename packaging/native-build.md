# Native release compilation

The release workflow builds every product binary twice on its native operating
system and architecture: a primary build and an independent build. It assembles
each role's own binaries and requires complete byte-identical archive/checksum
outputs before release attestation. Manual validation cannot create a release,
sign packages, or publish. Passing a development run is not final H4 qualification.

## Bounded native library and binary phases

The Intel macOS daemon and CLI exceeded the ten-minute job ceiling even with
four build jobs. The native x86-64 Windows daemon also exceeded that ceiling.
The daemon library producer and each final binary now have separate bounded
native phases on these hosts. Intel macOS adds a dedicated CLI-library phase
between its original daemon libraries and the final CLI binary. The other native binary commands, supported platforms, release profile, locked
dependencies, and compilation commands are unchanged. The binary matrix waits for
all independent library producers before starting; no role borrows another
role's compilation.

Release compilation jobs now allow twenty minutes, including setup, compilation,
library transport, and artifact upload. The previous ten-minute limit could expire
after Cargo completed but before its outputs were retained. The exception is limited
to the native library and binary producers, the manual previous-path compilation
comparison, and distribution product/check compilation. Assembly, qualification,
signing, and other hosted jobs retain their ten-minute ceilings. No timeout change
relaxes candidate binding, independent rebuilds, or required tests.

The existing library target is selected with `cargo build --release --locked
--package peritus-daemon --lib`. The final phase still runs the corresponding
`--package peritus-daemon --bin peritusd` build for the daemon, or
`--package peritus-cli --bin peritus` for the CLI. The CLI retains its own
dependency features: `cargo build --release --locked --package peritus-cli --lib`
recompiles feature-affected libraries in the intermediate stage when necessary.
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
cargo xtask release-cli-library
# In another fresh checkout, restore only this role's target/native-cli-libraries:
cargo xtask release-cli-binary
```

These commands reject existing compilation/product outputs. They use the fixed
`target/native-daemon` Cargo directory. Linux can exercise the transfer mechanics
locally, but Linux timing is not evidence of native macOS or Windows capacity.
Windows uses only the daemon-library and daemon-binary stages, with two build
jobs, pinned native clang-cl 20.1.8, and the original `cargo rustc ... --bin
peritusd -- -C link-arg=/Brepro` final command. The actual `.exe` bytes are
retained; no post-link rewriting or prebuilt executable reuse is allowed.

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
Intel macOS assembly requires exactly these two same-run, same-role records,
bound to their respective package and binary. The CLI library record must retain
the exact original daemon library as `previous_library`; daemon library records
must not have a parent. All dependent invocations must be distinct and ordered.
Windows x86-64 assembly requires exactly its daemon record and rechecks the
pinned C compiler identity. Assembly checks hashes against actual tar/ZIP
members, not loose package projections. Library records use schema v2, binary
records v3, and native assembly v4 retains them in `binary_compilations`.
Other platforms retain an empty map. Earlier development observations do not
qualify a new candidate under this protocol. The independent comparison remains
mandatory. Reruns must include producers and consumers from the same workflow
attempt; retrying just a consumer against an older attempt is intentionally rejected.

## Verification

`cargo test --locked --package xtask` includes the Python transport, source-input,
compile-failure, archived-binary, and native workflow boundary regressions.
Real fresh-source transfer and unsplit-build byte comparison are also required
before a new workflow is considered ready. Hosted native completion must be
observed on the exact commit; local fixtures cannot establish its timing.
Manual dispatch additionally compiles the previous Intel Mac CLI path (from
its original daemon libraries) and the previous cold Windows daemon path, then
compares them with the actual staged products. These diagnostic binaries cannot
enter assembly: only a separate `native-staging-diagnostic` report is uploaded.
