# Test-retirement scope restoration — October 10, 2026

The earlier cleanup removed documentation, bundled skills, and packaging/release source outside the authorized scope. This replacement was reconstructed from pre-removal commit `b3437fa0dc74cf42269f38f5155f182806efb015`. Real-defect fix commit `dc86dbc8d2a931c9168c9ef5f20dbf84d481178c` was cherry-picked as `426b7d7c0097c61cd0e5d5046fcc20602c8da80c`; its only conflict was resolved by preserving the fixed baseline-aware patch export and the original surrounding source.

## Preserved

- All 333 pre-removal Markdown files outside the dedicated formal-verification directory, including the original README and changelog history.
- All 57 defect-fix source paths, application crates, runtime safety checks, durable state and recovery, and the design/check/reviewer/fixer pipeline.
- The three product-role skills and the local 18-skill provider bundle. The provider bundle was originally ignored by Git and was restored separately; its commit skill retains the requested real-testing policy.
- Native packaging/installers, Linux distribution recipes, release inventory/SBOM/provenance/signing/publication tools, and the headless benchmark-agent interface.
- Existing protocol schemas, compatibility data, historical evidence, and real-testing reports/reproducers.

## Removed

Only positively classified unit/integration test sources and conditional test hooks, dedicated external test harnesses and qualification runners, repository verification commands, and formal proof tooling were retired. Verus macros were converted using the compiler's proof-erasure mode while retaining executable statements. Remaining explicit Verus-only source and compile conditions were removed. No solver or proof suite was run.

The `crates/app/testing` directory was reviewed by role: packaging/release code was retained, and only dedicated test harnesses were removed. The `xtask` crate retains its package, installation, native build, distribution, signing, and draft-release commands. The release operator now records the Rust toolchain without requiring retired Verus metadata.

GitHub Actions remains disabled, with zero repository-scoped runners. Dedicated test/formal workflows were removed; retained release build and signing jobs are explicitly disabled. Publication remains stopped.

## Observed behavior and evidence

The CLI, daemon, TUI, WebUI, sandbox helper, packager, release tools, and benchmark entrypoints built using `cargo build --offline --locked` with the supported Rust toolchain. The actual WebUI bundle built with `npm --prefix webui run build`. No unit tests, Clippy, repository checking suite, formal proof suite, or qualification campaign was run.

Focused real-interface operations exercised public CLI/daemon versions, CLI/WebUI help, required TUI arguments, retained package/release commands, rejection of retired commands, and the restored native packager. Its debug-artifact package contains the real rebuilt application and sandbox binaries; package version and artifact SHA-256 values are checked independently, and an invalid argument leaves the existing package unchanged. The earlier completed real-defect campaign is retained rather than repeated for unchanged fixes.

Full deletion reasons, source-preservation receipts, commands, outputs, and build logs are retained in `/home/doll/Peritus-cleanup-scope-restoration/`. The earlier defect replay evidence remains in `/home/doll/Peritus-real-defect-repairs-2026-10-10/`. These focused operations do not establish multi-platform release readiness or full application coverage.
