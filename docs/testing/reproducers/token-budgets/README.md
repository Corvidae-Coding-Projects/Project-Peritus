# Token-budget removal: real replays

Expected behavior: valid source-backed working state survives complete rendering and reopening
without a Peritus token allocation. Entries, dependencies, source locators, Unicode, role
isolation, and durable state remain intact. Provider context/transport capacity still applies.

Build and operate the public library consumer:

```sh
python3 docs/testing/reproducers/token-budgets/build_consumer.py /tmp/peritus-token-build
target/release/peritus-token-budget-consumer /tmp/peritus-token-case
```

Use fresh output directories. The consumer first reproduces the original explicitly capped
selection at 16,709 available / 16,903 needed. The same state must render completely through the
uncapped production API. It also operates empty state, 96 ASCII entries, 96 Unicode entries with
dependencies, all 512 storage entries, reviewer exclusion, cross-role rejection, and 24 generated
states (seed `0x50e7175`). It writes real observed source files, persists canonical working state,
reopens it, and compares entry identities, source references, provenance, and state bytes.
The concrete original numeric input is recorded in `exact-boundary-fixture.json`.

The host replay requires a private **closed Peritus backup**, containing `daemon/` and `config/`,
and a disposable copy of the stopped run's workspace. Never use a live store as its destination.
No credentials or original application source are included in this repository.

```sh
cp -a --reflink=auto ORIGINAL_WORKSPACE PRIVATE_WORKSPACE_COPY
python3 docs/testing/reproducers/token-budgets/prepare_host.py \
  CLOSED_BACKUP PRIVATE_HOST_DIR --run RUN_ID --workspace-copy PRIVATE_WORKSPACE_COPY
python3 docs/testing/reproducers/token-budgets/build_consumer.py /tmp/peritus-host-build --host
target/release/peritus-token-budget-host-consumer PRIVATE_HOST_DIR
```

Preparation copies SQLite through its backup API, copies immutable artifacts and the trace,
and retains only provider profile/executable metadata. The consumer recovers the real working
state and uncovered journal tail, keeps its exact instruction pins, calls the production
`LocalContextHandle::for_run`, `assemble`, and `checkpoint`, and reads back the checkpoint.
It uses the real configured Codex adapter's serialization and fit method. It makes zero provider
requests and dispatches zero application effects. Its configuration cases cover legacy token
keys at 0, 1, 4,096, 16,384, 16,385, and `u64::MAX`; output API cases check the uncapped default
and explicit ceilings above the former 32,768 maximum.

A successful host replay establishes local assembly and recovery. Actual provider inference and
retained-run continuation must be observed separately through the real client after deployment;
neither a fitting envelope nor successful compilation proves that continuation.
