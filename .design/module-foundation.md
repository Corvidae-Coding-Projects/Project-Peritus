# Feature: Product module foundation

## Outcome and boundary

Provide one usable registry/contract for bundled and locally installed workflow modules. Modules declare identity/version, description, roles/instructions, tools, ordered stages and explicit data dependencies. The host owns provider calls, effective permissions, private job storage, cancellation and recovery. Packages contain declarative workflow data/resources; installation does not execute package code. The existing process/Wasm plugin SDK remains the distinct native-code extension boundary and receives the identified lifecycle repairs; installing a workflow never routes arbitrary code through an unsandboxed child.

User decision: support both bundled and installable modules. Research will use gatherer → skeptic → synthesis → final skeptic → report. This task completes the shared workflow foundation and updates the research design; the separate full research feature retains its plan/Start, admission-ledger and report requirements.

## Ownership and integration

Add `peritus-modules` in orchestration for validated package definitions, immutable content identities, persistent enablement and run checkpoints. The product runner invokes module stages through `run_live_invocation`, the selected writer/reviewer providers and a read-only, permission-intersected tool catalog. Explicit `/module ID TASK` user input launches the host workflow directly; `/modules` lists available modules. CLI `peritus modules` manages list/show/install/enable/disable/remove. TUI and WebUI preserve the explicit commands. No module can widen workspace authority or silently invoke the coding pipeline.

Bundled source-brief supplies a real exercisable workflow rather than a demonstration executable. Installable packages use the same format and execution path. Scope here is read-only agent workflows with private outputs; workspace mutation/code plugins require their separate native capability/sandbox contract. The registry is not an alias for the unrelated E1/RSI materialization store.

## Data and recovery

Pin jobs to an exact module digest, current user revision and task. Each role has an independent memory lineage and receives only declared predecessor outputs. Checkpoint each stage output as an immutable artifact plus a small versioned manifest. An interrupted read-only stage is resumable; completed stage outputs are reused. Preserve outputs after module disable/remove; further execution rechecks enablement/digest and live permissions. Installation stages an immutable package and atomically publishes registry metadata under an interprocess lock. Invalid packages do not replace a working version. No lifetime token/request cliff; bound concurrency, individual frames and transient output pages instead.

## Alternatives

Only adding module prompts would not provide owned execution, lifecycle or state. Automatically loading arbitrary process plugins through the current raw child transport would bypass the product sandbox boundary. A host-interpreted workflow package is a complete useful module format for Research and keeps all effectful operations in existing host tools. Native third-party code remains explicitly separate until its actual sandbox integration is available.

## Real-interface acceptance specified before implementation

- Fresh CLI discovers the bundled module; install/inspect/enable/disable/remove a real local package, reopen in fresh processes, preserve exact digest/version/resources, reject traversal/symlink/invalid manifests and leave prior state intact.
- Run a real three/four-stage module through TUI/daemon and an actual provider; inspect per-stage artifacts, different role contexts, declared input routing, final result and live tool behavior. Exercise installable and bundled packages via the same path.
- Interrupt/restart/resume a module and observe committed-stage reuse. Reject disabled/changed modules and unavailable tools without workspace effects. Existing Chat/Design/web tools still work.
- Real plugin public consumer/process exercises cancellation while input is blocked/queued, malformed frames, stop/start, owned descendant cleanup, blocked-write deadlines and Unicode diagnostics. Stop only owned processes.
- Operate module controls in the real browser UI where available. No unit/mock/formal/repository verification suites. Build CLI/daemon with two Cargo jobs and observe available resources.

## Delivery

1. Package/registry/job contracts and CLI management.
2. Product workflow execution, context/permission intersection and live client commands.
3. Plugin lifecycle repairs, real-interface replays, documentation and commit/push to develop.
