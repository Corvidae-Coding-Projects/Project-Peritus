# GAP-03 versioned queue and recovery independent review

## Verdict

**Bounded PASS.** I found no blocking correctness, compatibility, API, or trust defect in the queue/version repair reviewed against base `a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8`.

This verdict is limited to the queue accounting, semantic version propagation, wire compatibility, durability/daemon recovery, schema consumers, clone contracts, and associated tests in this increment. It is an independent agent review, not human approval and not a claim that GAP-03 as a whole is complete.

## Review conditions and source identity

- Review was read-only. I made no repository source edit, build, commit, or push.
- The frozen implementation/test/fixture/generated-artifact snapshot is `target/gap3-evidence/92-versioned-queue-source.sha256`: 980 entries, manifest SHA-256 `401e634a71d72b827d97800d9875cf78fc83197ac1281621bd5ebf38224575bc`. I independently ran `sha256sum --check --quiet` successfully before and after the post-snapshot status-document update.
- I separately reviewed `docs/formal-scheduler-gap3/queue-version-design.md` at SHA-256 `e46b237787a95546e81358d75988530720ec3428d0714fbac97628dabaada153`. Its description of infallible current-frame constructors and encode-time `WrongSchemaVersion` rejection matches the code.
- During review I found that `docs/formal-scheduler-gap3.md` still named superseded evidence and said completed increment checks were in progress. The owner corrected that status block after the frozen implementation manifest. I re-inspected the corrected text; this did not change code, fixtures, generated artifacts, or the reviewed design.
- Any cancellation or later GAP-03 source change made after snapshot 92 is outside this verdict.

Selected implementation hashes from the frozen manifest:

| Path | SHA-256 |
|---|---|
| `crates/orchestration/peritus-scheduler/src/state/queue.rs` | `78cf4d6033ef886985bfe7057ea7361453bd6a018e6b4cc8b3f03f6d9b0b18ad` |
| `crates/orchestration/peritus-scheduler/src/semantics.rs` | `665b40258f0034e98aa1afecf21c1972c8ae3d9c064ea65f2015d9ed0d31cf5a` |
| `crates/orchestration/peritus-scheduler/src/durability/semantics.rs` | `fb26dac5a98f58e53786c8f99d303ad3a90eacf262a8dec13f348920906d0ca4` |
| `crates/orchestration/peritus-scheduler/src/canonical.rs` | `28630b2de70cc3fc48202fbc80015c73c66584714926826ba3d2beab92038659` |
| `crates/orchestration/peritus-scheduler/src/state/clone_impl.rs` | `59d547840533e7764fad2b59467600f28cb94c2f06452c88e9f4d09906094d49` |
| `crates/orchestration/peritus-scheduler/src/state/validation.rs` | `4820c40925a933f7d35fd178759e93e2f1c9289b23346cec611f2aa7f1730bd1` |
| `crates/orchestration/peritus-scheduler/src/wire/command.rs` | `08c5366e1dfc78a0e1e53d3cfaf33a31a485dacf371efbe0bc1a40bc33b2e2bc` |
| `crates/orchestration/peritus-scheduler/src/wire/event.rs` | `a07db18d7742075d6a9477655615d248ef60f1c81cf4b69155499dd858aee1e8` |
| `crates/orchestration/peritus-scheduler/src/wire/state.rs` | `241d73b81bef09519bc87181bd8f722af27ee180d4d001db28c26b775197ca63` |
| `crates/app/peritus-daemon/src/domain/dispatch.rs` | `af0cdbb09525a9976cd91a734b49f5a81f3407f2961afb2b67ba671d9163ab96` |
| `crates/app/peritus-daemon/src/command/service_tests.rs` | `683f5f5b9e8f12a6a4ec25b9b18348d3baa1d12cf62264275fb224c7ab7deffe` |
| `crates/foundation/peritus-protocol/src/schema/registry.rs` | `843cc924ddd52c0cda1671b57101f11dcb362645feba2bb310390c4f97ad1bdd` |

## Correctness findings

### Queue accounting and validation

The schema-2 pressure classifier is exact. It counts `Queued`, `WaitingDependencies`, and `RetryPending`, plus `Reserved` and `Running` work with `attempts_started < maximum_attempts`. The active classification deliberately applies to every recovery policy because an explicit retryable failure can make any such active item retry-eligible. `Cancelling` and active work already at the attempt limit do not consume strict pressure. The resulting checked relation is `W + eligible(Reserved | Running) <= Q`.

The schema-1 relation preserves the historical behavior: waiting work plus every `Reserved`, `Running`, or `Cancelling` item must be at most `Q + A`. The implementation counts a disjoint union and widens operands to `u128`, so neither overlap nor arithmetic saturation changes the decision. Legacy admission itself retains the old `W < Q` decision.

Decoded state validation applies the semantic-specific queue check before later identity/reservation correspondence checks. The hostile decoder tests therefore demonstrate that missing reservations, duplicate identities, or later structural errors cannot hide queue overflow. The exact `LimitExceeded` priority is checked before encoding and the real decoder rejects the consistently rehashed hostile state as `InvalidDomainValue`.

### Semantic identity, codecs, and historical bytes

Scheduler semantic identity is carried through bindings, commands, events, state, fences, replay, durable checks, canonical preimages, and state-derived callers. There is no implicit in-place upgrade. Public current-frame wrappers keep their infallible source API; encoding a wrapped legacy value through a schema-2 frame returns `WrongSchemaVersion` before bytes are emitted. Runtime-version callers use the semantic-aware helpers.

Families 70-72 advertise current version 2 and supported versions `[1, 2]`; other families remain current/supported version 1. App command binding, subscriptions, projection, and evidence test support membership rather than equality with the current version. Generated JSON and TypeScript publish current and supported versions while retaining the B3 artifact name.

The schema-1 payload readers remain shared but receive semantics from the checked header. Schema 0, schema 3, wrong families, nonzero flags, truncation, trailing bytes, and v1/v2 mixed histories are rejected. Schema-1 canonical preimages and the original fixture files remain unchanged. Independently observed v1 fixture hashes are:

- command: `632093d821d939617fb1b2b784fc905c27affd917dde2612600252df41eb211e`
- event: `1dd387ea8cd3fa3788596559e0abff04ab408b009b0e5ee84657dc0feb137e27`
- state: `9b913be2e0f46fe8205947bca1e3d6537899c433201d1aa7cc174f0217deee2b`

The v2 fixtures use a distinct domain/schema tag and stable distinct bytes. All three fixture manifests verify.

### Durability and daemon recovery

Exact command resolution occurs before the live-genesis semantic gate. Therefore an exact previously committed schema-1 genesis retry can return its original result, while a fresh absent-aggregate schema-1 genesis is rejected as unsupported without creating a head or checkpoint. Pending and indeterminate application records reconcile using the exact C0 command/result digests; a schema-2 relabel with the same command ID has a different digest and returns the existing application-level idempotency conflict rather than aliasing the v1 result.

Existing aggregate continuations derive semantics and fences from replayed state. Schema-1 continues as schema-1. Cross-version continuations, adjacent mixed-version events, and event/checkpoint semantic disagreement are rejected at the durability/replay boundary with no append. A supported but mismatched checkpoint version maps to `BindingMismatch`/`CorrectInput`; malformed or unsupported frame integrity remains a journal quarantine error. The pre-existing same-ID/different-digest behavior is preserved.

The three new daemon tests use fixed authentic legacy C0 command/event/state bytes and execute the real `command::submit`/`AuthorityOwner` path. They cover settled replay, both pending and indeterminate reconciliation, and same-ID v2 conflict followed by successful exact-v1 replay. Each asserts the original result/range or exact conflict and confirms that no extra global event was appended.

### Specifications and trust boundary

The semantic clone contracts compare every scalar field and ordered vector element used by scheduler state, work, bindings, events, and related values. They preserve direct array equality where required and do not weaken identity or order. I found no `external_body`, `assume`, `admit`, or equivalent trusted-method shortcut in the reviewed increment. The strict Verus run completes with `--no-cheating`.

The four guarded mutations provide useful non-vacuity evidence:

1. omitting `Running` from recoverable-active pressure fails a proof;
2. changing the attempt boundary from `< maximum_attempts` to `<=` fails a proof;
3. replacing the legacy occupancy classifier with the waiting classifier fails a proof;
4. routing schema 2 through legacy semantics fails a proof.

Each mutation produced 465 verified obligations and one proof error, exited 101, and was followed by byte-exact restoration. I independently compared the restored `state/queue.rs` to the three queue originals and `durability/semantics.rs` to the semantics original.

## Regression and qualification evidence

| Evidence | Observed result | SHA-256 |
|---|---|---|
| `62-base-history-export.log` | Protected unchanged base exported real v1 histories; exporter test passed | `2494d699b0361a884043355aec62eaf54e885223390d2a9f7d05a769e0126d00` |
| `68-schema-consumer-tests.log` | 144 passed, 0 failed, 0 ignored across protocol, app protocol, projection, and evidence | `c414cfd4079e87b4bf7d404e9c978a3c974f8585f7e8d56a108ae84950c95441` |
| `80-83-mutation-driver.log` | All four intended proof mutations rejected; originals restored | `34ea77df13a67d70fac3c4f89e3de95a4efc6ab3cc6f4bc9f2e711d468183730` |
| `84-restored-versioned-queue-verus.log` | 466 verified, 0 errors | `8a0aa087914b239b7879107f165aefd1899b9bdd604fbc9a91784c1d345c7ce0` |
| `85-final-versioned-queue-tests.log` | 62 passed, 0 failed, 0 ignored | `4f4a6901934365b9c6bb11755ae3808d1c1374fa1245f3d413bffcb817a65a0f` |
| `87-protocol-codegen-check.log` | Generated protocol check passed | `20a4f22c3266b904f2d38ff9af4b39c7ead8db8487afdcf9310be66730fdc875` |
| `88-workspace-check.log` | Workspace all-targets/all-features check passed | `0d178909e81ebf902d4906d290ec92ec11b1d410472634c6f4838b7594adae67` |
| `89-daemon-library-tests.log` | 178 passed, 0 failed, 3 existing ignored | `d72a1b6b6c130d402e6a85bb4261e56a3f8fcdd285feb983387eefbfc40cd323` |
| `90-format-check.log` | Format check exited 0; empty success log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `91-final-versioned-queue-clippy.log` | Locked strict Clippy passed for seven affected packages, all targets/features, `-D warnings` | `fc62783e94a6d168bbcb7844f285136c0f8c98c655559a03cc260deb7a8e41e4` |

The daemon run's three ignored tests are visible in the log: one subprocess fixture invoked by another test and two graphical tests requiring Xvfb/xdotool/ImageMagick/Python Tk. They are pre-existing and do not conceal a scheduler queue/version failure.

The 45 `scheduler-v1-recovery` frames were exported from an unchanged protected checkout of the base commit. The recovery and wire tests replay those fixed histories, verify the historical decisions/bytes, and exercise legacy continuation, exact retries, final-attempt behavior, all recovery policies, hostile queue states, mixed versions, and checkpoint/event mismatch.

## Remaining boundary

This increment proves the queue classifiers/count loops and semantic clone/version relationships and connects them to substantial ordinary and durability tests. It does not yet publish a single end-to-end theorem that every complete `start`/`decide`/`apply` reducer transition preserves the selected queue invariant. In particular, complete composition across all admission and ordering producers, cancellation and worker-loss loop iterations, retry/dependency refresh/exhaustion paths, and replay agreement remains open. The broader release/trust reconciliation and final GAP-03 caller qualification also remain open.

Those gaps prevent a whole-GAP-03 completion claim. They do not invalidate the bounded PASS for this queue/version increment.
