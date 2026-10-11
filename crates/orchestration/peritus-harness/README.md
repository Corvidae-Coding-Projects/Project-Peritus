# peritus-harness

`peritus-harness` is the durable E1 boundary for production harness definitions. It loads the
strict `.peritus-harness/manifest.toml` entry point only through C1 no-follow inspection, checks
the complete component graph, constructs immutable content-addressed revisions, and plans exact
owned-path materializations.

The crate separates pure domain work from effects. A materialization is committed to C0, including
its checkpoint, artifact roots, idempotency binding, and outbox directive, before the runtime asks
C1 to apply one checked `PatchSet` and create an immutable candidate. E1 never grants authority,
writes repository files directly, invokes Git directly, evaluates a harness, or promotes one.

Canonical schema-v1 command, event, and checkpoint frames use families 79, 80, and 81. Harness
aggregate checkpoints use namespace `0xE101`; C0 aggregate tag 13 is reserved for the harness
aggregate.

The main registration path is `load_harness` → `LoadedHarness::check` →
`CheckedLoadedHarness::{finalize_artifacts,genesis,successor}`. Finalized component roots must exist
before `commit_harness_transition` can register or plan a revision. `HarnessRuntime::commit_plan`
returns a `CommittedPlan` type-state value; only that value can enter `execute_claimed`, which
requires an exact claimed C0 outbox message and atomically records success/failure while settling
its claim. `materialization_authorization_payloads` produces the exact inert patch and predicted
candidate payloads needed to acquire both separate authorizations without reimplementing E1 patch
construction. After restart, `HarnessRuntime::recover_plan` recreates that type-state only from the
same C0 store's checked contiguous replay and exact matching checkpoint.

Ordinary run APIs keep the shared seven-field `RevisionTuple` unchanged. The read-only
`GoverningHarnessBinding` pairs it with the exact E1 revision identity and matching materialization
receipt/snapshot, rejecting lineage, digest, or C1 snapshot disagreement without exposing a
mutation route.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-harness
```
