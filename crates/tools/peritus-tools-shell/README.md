# peritus-tools-shell

`peritus-tools-shell` supplies the built-in `shell.exec` and `shell.script` descriptors and their
authorized C2/C3 dispatch adapters. Structured argv is the default. Script text is accepted only
by the separately named, higher-risk script descriptor.

Both tools require a precompiled restricted `ExecutionPlan`, matching C2 authority, a checked C3
sandbox plan and admission, and a concrete native backend. There is no unrestricted execution
fallback. Running processes remain owned by C2 while this crate maps their ordered observations,
controls, artifacts, terminal states, and recovery into C4 envelopes.

`ShellDispatcher` is the restricted production adapter. `RawShellDispatcher` serves callers that
have already compiled and authorized an explicit raw-effect plan, including G4's workspace-scoped
developer commands. It does not weaken the default descriptor or create a separate process path:
both dispatchers launch through `ExecutionGateway`, retain output in the artifact store, and expose
the same C4 poll, stdin, resize, signal, cancel, and recovery protocol.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tools-shell
```
