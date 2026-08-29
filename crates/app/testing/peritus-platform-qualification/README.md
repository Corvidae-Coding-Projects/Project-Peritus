# peritus-platform-qualification

`peritus-platform-qualification` owns H2's target-independent contracts for qualifying a staged
Peritus release on fresh Linux, macOS, and Windows subjects. It validates release layouts,
checksummed package manifests, foreground-daemon supervisor definitions, local IPC expectations,
native sandbox requirements, process-equivalence observations, lifecycle ownership, bounded
evidence, and the final ready/not-ready decision.

The crate performs no installation by itself. An H2 adapter implements `FreshSubjectFactory` and
`QualificationSubject` for the selected VM or runner. `FreshSubjectRunner` creates a different
subject for every closed scenario, requires complete cleanup, and returns evidence that
`QualificationReport::evaluate` reduces without consulting ambient host state.

The reviewed assets in `../../../../packaging` are the native application layer consumed by a
release builder. They supervise the real `peritusd serve --config <absolute-file>` invocation;
they do not add an unimplemented daemonization or service command. Configuration, state, and logs
remain operator/runtime owned and are preserved during upgrade, rollback, and ordinary uninstall.

This crate is an H2 qualification foundation, not evidence that any package or host has passed.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-platform-qualification
```
