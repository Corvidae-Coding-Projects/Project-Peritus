# Workbench dependency execution review

Scope: the new pinned `cap-std`/`cap-fs-ext` filesystem substrate and `image` decoder closure.
The crates.io archives below were hashed locally; every SHA-256 matched the exact checksum in
`Cargo.lock`. The corresponding unpacked `build.rs` was then read in full.

| Package | Locked archive SHA-256 | Executed inputs and effects |
| --- | --- | --- |
| `cap-fs-ext@4.0.3` | `56ff379b70af8e08307a8f65e7040c7301cb4a572538ade16b4984f0da77847f` | Reads `RUSTC`, `TARGET`, optional `RUSTC_WRAPPER` and encoded rustflags; compiles an inline `windows_by_handle` probe and emits exact cfg/check-cfg directives. |
| `cap-primitives@4.0.3` | `8b5f74729fd2f44701d1a8eb47e906cdb3ccd9ec0f02baad85a744b791940b18` | Uses the same inline compiler probe for five named library features and declares three caller-set cfg names. |
| `cap-std@4.0.3` | `c1ec78e242cfa2cfe276807ac2ecc00315a6c97786977414bcd1c3963b6c91b8` | Uses the same inline compiler probe for three named library features and declares `io_lifetimes_use_std`. |
| `crc32fast@1.5.1` | `8498c871161e1742aaa9d52551b2d6ebdd4c3d45a3be423e3728f33b955be550` | Executes `RUSTC --version`, parses the minor version and emits only the two documented intrinsic cfg names. |
| `io-extras@0.19.0` | `20fd6de4ccfcc187e38bc21cfa543cb5a302cb86a8b114eb7f0bf0dc9f8ac00f` | Compiles two inline feature probes through `RUSTC` or `RUSTC_WRAPPER`, writes metadata only under `OUT_DIR`, and enables `io_lifetimes_use_std`. |
| `io-lifetimes@2.0.4` | `06432fb54d3be7964ecd3649233cddf80db2832f47fec34c01f65b3d9d774983` | Reads the target OS; only for WASI, compiles an inline `wasi_ext` probe through `RUSTC` or `RUSTC_WRAPPER` into `OUT_DIR`. |
| `io-lifetimes@3.0.1` | `2f0fb0570afe1fed943c5c3d4102d5358592d8625fda6a0007fdbe65a92fba96` | Same bounded WASI-only probe as 2.0.4. |

No script contains network access, repository traversal, C/C++ compiler or linker invocation,
or shell-command construction. The retained concerns are build-time execution of the configured
Rust compiler/wrapper, compiler- and target-dependent cfg output, and feature-probe metadata
written to Cargo's `OUT_DIR`. The trust gate therefore permits only these seven
source/name/version identities; version and registry near-misses remain rejected.

The cargo-deny reconciliation is likewise exact: `winx@0.36.4` alone receives its published
Apache-2.0-with-LLVM-exception license, while only `io-lifetimes@2.0.4`,
`windows-sys@0.52.0`, and `miniz_oxide@0.8.9` receive documented duplicate-version skips.
Global multiple-version denial and all advisory/source restrictions remain unchanged.
