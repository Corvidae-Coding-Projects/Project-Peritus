# peritus-platform-qualification

`peritus-platform-qualification` owns H2's target-independent contracts for qualifying a staged
Peritus release on fresh Linux, macOS, and Windows subjects. It validates release layouts,
checksummed package manifests, foreground-daemon supervisor definitions, local IPC expectations,
native sandbox requirements, process-equivalence observations, lifecycle ownership, bounded
evidence, and the final ready/not-ready decision.

The crate includes the shared native adapter and one-command operator used by Linux, macOS, and
Windows qualification runners. `NativePlatformFactory` stages and re-digests the exact manifest
artifacts into a different private subject for every closed scenario. It invokes one reviewed
native controller under a bounded process tree and private user environment, validates exact
request, subject, scenario, controller, package, and target bindings, verifies retained raw
artifacts, and requires separately bound cleanup evidence. `FreshSubjectRunner` then returns the
complete observations that `QualificationReport::evaluate` reduces without consulting ambient
host state.

`peritus-h2-controller` owns the actual platform effects: package installation, native supervisor
and IPC exercise, process and sandbox probes, upgrade and rollback, uninstall, and resource
cleanup. It rejects a request unless the target, package, manifest, staged controller, subject, and
limits match its current invocation. The versioned request, scenario-response, and cleanup-response
schemas live in `packaging/schemas/`. A separate fixture controller proves the adapter and operator
protocol; fixture results are not release evidence for a real host.

The reviewed assets in `../../../../packaging` are the native application layer consumed by a
release builder. They supervise the real `peritusd serve --config <absolute-file>` invocation;
they do not add an unimplemented daemonization or service command. Configuration, state, and logs
remain operator/runtime owned and are preserved during upgrade, rollback, and ordinary uninstall.

## Operator command

Build a target package and the standard native controller, create empty scratch and
retained-artifact directories outside the repository, then run:

```sh
CARGO_BUILD_JOBS=2 cargo build --locked \
  --bin peritus-h2 \
  --bin peritus-h2-controller
target/debug/peritus-h2 \
  --controller target/debug/peritus-h2-controller \
  --package /path/to/peritus-package \
  --manifest /path/to/peritus-package/manifest.toml \
  --scratch /path/to/private-h2-scratch \
  --artifacts /path/to/new-h2-artifacts \
  --report /path/to/new-h2-report.json \
  --platform linux \
  --architecture x86_64 \
  --version 6.6.0
```

The operator refuses to overwrite a report. It exits successfully only for `Ready`, exits 3 for a
completed `NotReady` campaign, and reports adapter or evidence failures as errors. Linux, macOS,
and Windows must each run the native controller on a fresh supported host. The checked-in fixture
is protocol evidence only; no package or host is claimed qualified by its test result.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-platform-qualification
```
