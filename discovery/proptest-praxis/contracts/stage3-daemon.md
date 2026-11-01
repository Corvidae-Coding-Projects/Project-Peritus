# Stage 3: real daemon steering and recovery

`owner/daemon*.rs` creates disposable state and launches the actual daemon runtime in an owned child process of the real `peritus-daemon` test executable. A configured, revision-bound recording provider replaces only the inference dependency under `cfg(test)`. The normal daemon startup, migration, approval-registry installation, authority epoch, OS peer authentication, server, application client, control journal, workspace admission, product coordinator and ordinary tools execute unchanged. Every case has its own synthetic store/actor/workspace identities and paths. An empty valid public approval registry supplies no private keys or extra authority.

The parent operates `peritus-app-client::Client` over the actual Unix socket, including reconnects. It inspects committed queue revisions, provider request bytes, interaction state and exact immutable public replies. The model records and barrier markers are atomically published, preventing partial fixture writes from becoming apparent application failures.

## Predeclared schedule oracles

- **Before tool admission:** release the provider response, pause at a test-only barrier immediately before `LiveConversation::admit_tool` checks C0 generation, accept/edit/hold/withdraw inputs through IPC, then release admission. The stale child must yield. Actual host activity must not show the stale tool as executed; current eligible bytes must reach the following request. Held/withdrawn sentinels must remain excluded, and unholding must restore only that exact input on a subsequent turn. This is host-boundary evidence; the read-only tool has no external mutation counter.
- **Before response finalization:** accept inputs while the old provider stream is blocked. The subsequent real request must contain the latest exact eligible revision. At idle, accepted/incorporated generations must agree. The brief-page/proposal APIs must expose exactly one immutable committed reply, with exact expected bytes, size and digest. Already streamed Assistant activity is permitted and is not mistaken for a terminal C0 reply.
- **After runner return:** pause immediately before daemon terminal publication, accept another input, then release. The coordinator must automatically retry accepted pending work; the next real request must contain it and eventual incorporated state must account for it. This supplements the earlier response boundary and the domain-driver completion crash cuts.
- **Model/effort changes:** an in-flight request retains its initial selection; an update plus steering in that same active invocation affects the next request. A later idle continuation preserves the newly selected model/effort. A rejected unsupported choice cannot mutate durable role selections or substitute a future request. The current public empty-choice contract explicitly selects the configured profile; this is tested as documented behavior, not silently changed into another policy.
- **Cancellation/retry:** current cancellation reaches an honest Cancelled state, then an explicit retry creates a new request and reaches idle. Cancellation is not inferred from a transport EOF.
- **Provider crash:** terminate the actual owned daemon process, start a fresh daemon against the same state, reconnect a fresh IPC client and inspect RecoveryRequired. Startup itself must not issue a new provider request. Explicit retry must resume work truthfully.
- **Completed ordinary effect:** direct folders enter through ordinary Chat `run_pipeline`, with real grounding, design handoff and exact `workspace_scope`. The real command uses literal `dd` arguments, an immutable fixture input and an append target. Both successful C2 metadata and exact independent output bytes are required. Kill after a subsequent provider boundary, reopen and explicitly retry; ordinary source inspection must preserve exactly one counter line. The recovery tail establishes retained effects/read behavior, not acceptance of a fully reviewed original pipeline deliverable.
- **Idle workspace reuse:** an idle conversation must not prevent a fresh conversation in the same workspace from admitting an actual provider turn.

The expected scheduler prefix and state distinctions are defined by these cases, not generated from returned phases. The actual provider emits valid normalized C5 events, unique per-turn call identities and explicitly released responses. It records actual ModelRequest model/effort/message fields before responding. It supplies no claimed filesystem success. Actual effects must be independently visible in the workspace and ordinary host result.

## Calibration

`stage3-daemon.patch` deliberately removes the current-generation fence at ordinary child admission. The fixed witness pauses after the generic developer loop's earlier checks, so newly accepted input reaches the actual C0 admission fence. This avoids a canary that survives merely because an earlier check makes the mutated branch unreachable. Generated and shrunk cases must detect the resulting stale dispatch. The patch is applied only in the detached calibration checkout and is restored exactly afterward.

## Grounding and limits

Implementation/caller entrypoints inspected include `product_run/{construction,catalog,lifecycle,interaction,execution}.rs`, `interaction/{models,live}.rs`, `interaction/live/request.rs`, `execution/launch.rs`, `workbench/{execution,inputs,conversation}.rs`, `product_control/{storage,replies}.rs`, daemon startup runtime/component registry and actual IPC/client facades. Supporting exact reply/queue DTOs and ordinary conversation/tool/pipeline boundaries were inspected where the fixture uses them. This record names selected entrypoints, not complete source coverage for those packages.

Evidence is Linux x86_64 only. The recording provider is not live inference, provider transport/credentials, or a model-semantic evaluation. Windows named pipes, power-loss durability, arbitrary OS ACL/symlink races, all workspace kinds, every permission combination, real reviewed pipeline acceptance, all cancellation race positions and unrelated module workflows remain outside this bounded campaign. Model source/availability metadata beyond the immutable selected profile is not proven. Domain AgentDriver recovery is separately exercised and is never substituted for ProductRunService recovery.

The model-change case records canonical effort values and compares exact `low`/`high` values;
`xhigh` cannot satisfy `high`. It verifies both steering sentinels in actual requests, exact
incorporated queue revisions, received/incorporated accounting and the first immutable final
reply. Request identities must share their invocation prefix after parsing only the documented
turn/attempt suffix. Live selection assertions exercise the writer; assigning the same choices
to all roles does not establish independent reviewer/fixer selection or switching provider
implementations. The schedule selector ignores the generic history-length flag: its nine
explicit finite scenario families, edits and reconnect choices are the actual generation bounds.
