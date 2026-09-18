# Local daemon investigation — 2026-09-12

The project-tab close feature is implemented separately from these findings.
These observations come from read-only daemon queries, process inspection,
retained configuration/run records, and the source in this checkout. No live
run was retried and no provider inference was requested during diagnosis.

## Findings

### 1. The installed update is not the running daemon

PID 774545 answered `DaemonStatus` with `ReadyReadWrite`, but Linux reports its
executable as `peritusd (deleted)`: the installed file was replaced while the
process retained its old executable. SHA-256 values differ:

- Running: `1c3ad1efcccf1db6ca2f475260f71041c0c208a73225911e10c46ea11969049f`
- Installed: `8d1b5b5c1d7fb5ffae627d873d3def1adb2f94274f9e25f7d1ebb850b48a6d49`

Rebuilding/installing alone has therefore not activated the new daemon code.
The installed binary's source revision was not established by this inspection.
The launcher has replacement/restart detection in
`crates/app/peritus-launcher/src/daemon.rs`; the WebUI only connects to an
existing endpoint and does not perform that launcher lifecycle.

### 2. Direct-folder configuration only includes the active folder

The running process uses configuration generation 12. It contains one folder:
**Peritus WebUI**, workspace `f55c359f916d2217c881badfa3b1c7cd`.
Product state additionally remembers **Peritus TUI** and **Peritus Runtime
Restarts** as trusted direct folders, but neither appears in that configuration.

`crates/app/peritus-launcher/src/bootstrap/configuration/folder.rs` renders only
`state.workspaces().active()`. The daemon builds its execution-root map from
the loaded workspace catalog in `product_run/construction.rs`. New execution
and retry require a matching root in that map (`product_run.rs` and
`product_run/lifecycle.rs`). Remembering a trusted folder does not make it
available to this running daemon.

This explains the unavailable-workspace path for the retained TUI run and
prevents reliable simultaneous operation across these direct folders.

### 3. Automatic recovery hides failure

Run `7ba609e20c65bbdd2348b7ec4e13610e` remains `RecoveryRequired` while its status
says "Daemon restart interrupted this run; continuing automatically".

`crates/app/peritus-daemon/src/product_run/lifecycle.rs` writes that message
during shutdown. `resume_interrupted` later discards the result of
`self.retry(run_id).await`. A failed retry can leave the promise of automatic
continuation intact. The missing workspace above is sufficient to prevent this
run from resuming under the observed configuration; the precise historical
retry error was not retained.

`product_run/error.rs` also maps both workspace and provider unavailability to
`StaleRevision` without a detailed diagnostic. This obscures useful recovery
instructions at the client boundary.

### 4. The older WebUI run failed in local context assembly

Run `e100d24b15d43a5fcf2ad739a28ab15b` records:

> prepare developer context: local working memory: required working-state
> closure exceeds input capacity

The path is `crates/app/peritus-product-runner/src/local_context/assembly.rs`.
Its structured working-state budget defaults to **4,096 tokens** and is capped
separately from the provider's **200,000-token** input limit. Required working
entries include unresolved plans, failed approaches, contradictions, and their
dependencies (`crates/orchestration/peritus-context/src/working/selection.rs`).

A read-only inspection of the last published checkpoint, with its validation
artifact checked against its SHA-256 digest, found:

- Estimated model input: **73,033 / 200,000 tokens**.
- Uncompacted input estimate: **201,834 tokens**.
- Local compactor failures: **0**.
- There were **31 journal events** after that published view's `through_event`.

That successful prior view does not describe the failed next view. It does show
why the error should not simply be diagnosed as a provider connection failure
or proof that the full model context was exhausted. The assembly code collapses
render errors into the same capacity message; the exact failing render reason
and required-entry budget need to be preserved before prescribing a memory fix.

### 5. WebUI setup/readiness has additional gaps

- **Project-Peritus** points at the child repository, while the registered
  direct folder is its parent, **Peritus WebUI**. `peritus-web/src/daemon.rs`
  matches roots exactly, so this browser project correctly has no matching
  registration under the current rule.
- **Peritus TUI** is shown as registered from product state even though the
  running configuration does not include it. The WebUI's facts query does not
  establish that a remembered workspace is currently executable.
- **Peritus TUI 2** no longer exists and has no matching registration.
- The browser run-history panel can inspect runs from other clients; it does
  not attach those runs to a native browser conversation. Existing CLI
  conversations require selection in the CLI.

### 6. The daemon log is mostly disconnect noise

At inspection, all 628 log lines were application-connection termination
messages: 619 read closures and 9 failed writes. The WebUI opens a connection
per request and drops it afterward (`peritus-web/src/daemon.rs`); the IPC server
logs connection termination (`peritus-daemon/src/ipc/server.rs`). These lines
alone do not demonstrate 628 daemon failures. Read-only status and run-list
requests succeeded during diagnosis.

## Repair order

1. Retain the intended direct-folder execution scopes across project selection,
   with per-workspace handling of moved/deleted folders. Test two concurrently
   usable folders and an unavailable third without blocking the healthy ones.
2. Persist an actionable reason when automatic recovery fails; claim resumed
   execution only after retry succeeds. Test missing-workspace/provider startup
   recovery without invoking a real model.
3. Separate remembered registration from live daemon readiness in the WebUI and
   clearly explain exact-root setup mismatches.
4. Preserve typed working-state render diagnostics and add a bounded recovery
   path for required-entry overflow. Repeatedly retrying the same memory state
   is not a remedy, and silently deleting required context is not acceptable.
5. Activate the intended daemon build with a controlled restart once its
   workspace configuration and recovery behavior are understood. A restart by
   itself does not fix the single-folder configuration in this checkout.
