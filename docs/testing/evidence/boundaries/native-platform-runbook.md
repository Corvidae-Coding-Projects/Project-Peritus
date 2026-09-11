# Native process and lifecycle completion runbook

These commands are suggestions for disposable hosted runners. They were not executed by the local boundary campaign and do not authorize workflow edits, pushes, or live operator-profile mutation.

## Existing hosted matrix

The reviewed workflows already provide `macos-15`, `macos-15-intel`, `windows-2025`, and `windows-11-arm` native package jobs in `.github/workflows/product-package.yml`. The same-run path builds native artifacts, runs `product-native-qualification-prepare`, restores them on the qualification runner, and executes all `product-native-qualification-prepared-shard` indices. `.github/workflows/ci.yml` and `formal-governance.yml` also route terminal interactive, signal, and cancellation shards to macOS and Windows.

On each clean native runner, use the repository-pinned toolchain and two compilation jobs:

```text
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-process
CARGO_BUILD_JOBS=2 cargo run --locked -p xtask -- product-native-qualification
```

On Windows PowerShell 5.1, additionally run the issue-59 lifecycle harness against the checked-out repository:

```text
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File packaging/tests/windows-lifecycle.Tests.ps1 -RepositoryRoot $PWD
```

For the prepared same-run workflow, follow the existing job sequence instead of inventing artifact paths:

```text
cargo run --locked --package xtask -- product-native-qualification-prepare
cargo run --locked --package xtask -- product-native-qualification-restore
cargo run --locked --package xtask -- product-native-qualification-prepared-shard <0-through-17>
```

Retain each H2 report plus runner OS/architecture, commit SHA, exact command, exit status, process census, and preserved-root digest. macOS must confirm the real `launchctl` absent-job exit classification before accepting the shell regression. Windows must record exact executable paths for every stopped process and prove the sibling fixture remains live. A fake supervisor result cannot substitute for either native observation.
