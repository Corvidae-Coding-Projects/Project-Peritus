//! Human-readable command inventory, separate from argument parsing and execution.

pub(super) const HELP: &str = "Peritus workspace policy tool

Usage: cargo xtask <command>

Commands:
  all                    Run all locally executable repository policy checks
  architecture-check     Validate packages, layers, ownership, and source layout
  docs-check             Validate maintained Markdown structure and local links
  format-check           Check every workspace package without one oversized command line
  ordinary-api-check     Validate formal APIs callable from ordinary safe Rust
  source-layout-check    Validate module names, crate roots, and source budgets
  reproducibility-check  Validate toolchain pins, lock policy, and immutable CI inputs
  toolchain-check        Probe installed Rust, Verus, vstd metadata, and bundled Z3
  verify-trust           Reject trusted Verus constructs outside approved roots
  ci-shard OPERATION SHARD Run one reviewed package shard for hosted Rust or Verus CI
  product-package        Build a host-native checked Peritus package in dist/
  product-install        Build and install Peritus for the current user
  product-package-smoke  Qualify native install, repeat launch, upgrade, and uninstall
  product-native-qualification Run and retain all 18 native H2 package scenarios
  product-native-qualification-shard INDEX Run one of 18 single-scenario H2 shards
  product-native-qualification-prepare Assemble previously built native H2 artifacts
  product-native-qualification-restore Restore this platform's same-run native H2 archive
  product-native-qualification-prepared-shard INDEX Qualify same-run artifacts without Cargo
  release-bootstrap-smoke Qualify the public download, checksum, and install entry point
  release-bootstrap-prepared-smoke Qualify the public installer using same-run native artifacts
  release-bootstrap-staged-smoke Qualify the public installer using the exact staged archive
  release-qualification-prepare Restore the staged release archive and retain H2 inputs
  release-rebuild-record Retain actual source, environment, and native assembly observations
  release-rebuild-compare Require a compatible byte-identical independent native rebuild
  release-daemon-library Compile and retain a same-run native daemon library tree
  release-daemon-binary Compile the final daemon from its verified same-role library tree
  release-cli-library    Compile CLI libraries from verified same-role daemon libraries
  release-staging-check  Compare staged products with the previous native build path
  release-cli-binary     Compile the CLI from its verified same-role CLI libraries
  release-windows-binary Compile a selected native Windows x86-64 binary with pinned C tooling
  release-windows-sqlite-check Compare two fresh native Windows bundled SQLite compilations
  release-create         Validate a tag and create its retained draft GitHub release
  release-package-stage Build, archive, checksum, and record this host's native package
  release-package-assemble Assemble a native package from separately built release binaries
  release-stage          Complete and validate the release draft without publishing it
  distro-image           Build a pinned Debian or Fedora package-builder Docker image
  distro-image-save      Retain the exact builder image for same-run package jobs
  distro-image-restore   Restore the same-run builder image without rebuilding it
  distro-build           Build real source and binary distribution packages offline
  distro-compile         Retain a candidate-bound native distribution compilation
  distro-compile-checks  Retain native RPM check compilation for final recipe execution
  distro-package-compiled  Run full recipes and tests on the same-run compiled tree
  distro-sign            Sign distribution packages using the local OpenPGP agent
  distro-sign-ci         Sign with explicitly provisioned protected-environment CI secrets
  distro-verify          Verify package signatures and disposable install/remove lifecycle
  distro-upload          Upload a verified package set to the exact existing release draft
  distro-test            Run distribution package tooling regression tests
  discovery-replay       Replay every registered discovery corpus on stable Rust
  discovery-posix-lifecycle Qualify strict disposable POSIX uninstall failure handling
  discovery-setup-fuzz    Install exact harness-only nightly and cargo-fuzz pins
  discovery-setup-mutation Install the exact cargo-mutants pin
  discovery-mutation-SLICE-N Run fixed shard N (receipt/context 0-7; cancellation 0-11)
  discovery-mutation-context Inventory required-context mutations; zero reachability fails
  discovery-mutation-context-canary Prove the required-context capacity test rejects its curated mutant
  discovery-mutation-receipt Inventory and test effect-receipt mutations
  discovery-mutation-cancellation Inventory and test product lifecycle mutations
  discovery-fuzz-sse      Run bounded SSE fuzzing and retain completion evidence
  discovery-fuzz-ndjson   Run bounded NDJSON fuzzing and retain completion evidence
  discovery-fuzz-provider-sequence Run bounded structured provider-event fuzzing
  discovery-fuzz-working-state Run bounded structured working-state fuzzing
  help                   Print this help
";
