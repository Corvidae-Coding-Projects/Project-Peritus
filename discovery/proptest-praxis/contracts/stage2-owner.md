# Stage 2: interactive product-context mechanisms

The discovery-only `cfg(test)` modules run inside the real `peritus-product-runner` owner. No production type becomes public to support investigation. The one constructor added to `LocalContextHandle` exists only in test builds. Every fixture owns a disposable workspace, actual SQLite journal, immutable artifact store and original C5 messages; no live account, model, daemon, browser, user archive or project file participates.

## Read reuse

`owner/reads.rs` drives `MemoryTools` over `WorkspaceDeveloperTools::read_only`, including the normal permission, task-source policy and freshness admission. It archives actual proposals, outputs and tool messages through `LocalMemory`, then changes the exact final visible view using the real `context_read` reader and `observe_model_context` boundary. Reopen releases and reacquires the real store owner; it does not serialize a fake memory structure.

The independent model is a map from immutable content digest to a **set of individual original-file byte positions**. It never uses the production interval-merging or gap-selection algorithm. Returned labelled bodies must exactly match the original file, reported half-open coverage must equal the delivered body boundary, all returned positions must belong to the independently requested extent, reused missing intervals cannot overlap currently visible bytes, and completion cannot omit requested bytes. An `already_visible` receipt must name the entire requested range and its positions must already be in the model's current view. Every archive delivery is checked against the exact original canonical bytes before it earns model credit. New file versions have separate visibility sets, even when their lengths match.

Generated and fixed histories include partial line continuations across Unicode, combining characters, escapes, empty lines, overlapping ranges, view replacement, old-page restoration, same-size file edits, current read permission changes, opaque task-source constraints and real store reopen. The permission and source checks exercise the actual executor. A view's `protected_paths` are leave-alone **mutation constraints**, so they are not misrepresented as read revocations. The source-restriction fixture instead changes the real task's black-box/opaque-source contract.

The canary omits `visible.clear()` at view replacement. It must cause the independent model to reject a receipt crediting bytes absent from the current model request. This is a deliberate defect, not a discovered product bug.

## Decision projection and complete request accounting

`owner/decision.rs` ingests ordinary actual workspace reads, repeated equal reads and distinct failed reads. Its independent map records the first identical successful `(tool, original arguments, exact output)` and retains each error separately. It compares both incremental and freshly rebuilt real `Index` instances to that map, including representative handles and exact original output bytes. Root-hash equality between incremental and rebuilt paths is supplementary; it is not an independent hash-format proof.

It calls the actual `prepare_view_with_governing` assembler under History/Conservative, plans enabled/disabled, Writer/Fixer/Reviewer, and unrestricted/revoked-read/opaque-source states. Current system policy, user input and host invocation state must survive literally with their original roles. Successful conservative cards after same-size edits must report changed content. Excluded conservative file evidence must not appear as source text.

Each emitted card's **advertised** source handle, tool, error flag, byte count and original arguments are checked against the original exchange. Its exact visible prefix and UTF-8 continuation offsets are checked against the original bytes. Continuation retrieval uses that advertised handle through the real `context_read` entrypoint and reconstructs the exact original bytes; the harness does not substitute a correct handle for a broken advertised one. Tool definitions participate in the actual request. Published estimate fields must describe that entire final message/tool projection. Changing the separate output-token allowance while preserving the input ceiling must leave the exact projection unchanged, checking against double subtraction of output capacity.

The alias canary records an identical inspection as its own representative. The independent first-occurrence model must reject it.

## Source and caller grounding

The source/caller paths inspected for these checks are `local_context/{port,memory,read_reuse,source_policy,tools,assembly}.rs`; `memory/{environment,ingestion,checkpoint}.rs`; `read_reuse/{page,view}.rs`; `assembly/{evidence,validation,retrieval}.rs`; `assembly/decision/{mod,index,plan,bindings,dependencies,render}.rs`; `tools/read.rs`; and the actual `developer_tools/{executor,access_policy,inspection_read}.rs`, `executor/{construction,read_reuse}.rs`. The public context pilot in Stage 1 exercises the separate domain working renderer, not this interactive request assembler. The plan contract is separately recorded in `stage2-plans.md`; public invalidation/knowledge and memory contracts are recorded in their own Stage 2 files. These records identify mechanisms and selected supporting paths, not whole-package review coverage.

## Bounded conclusions

These are serial library/owner tests with real filesystem and local persistence effects. They do not establish concurrency/crash durability, native ACL changes, symlink races, external absolute-reference behavior, live provider encoding/tokenization, answer equivalence, global optional-selection optimality, all working-entry histories inside the product assembler, or end-to-end daemon/client behavior. Accounting checks correspondence to the production estimator; independent tokenizer accuracy is not claimed. Missing/oversized plan archives, stale cursors and permission changes between children are covered by the separate real-plan coordinator campaign. The combined read→decision→plan→restart integration campaign remains Stage 8 work.

Counters count actual checked actions and reached branches; `max_` counters are maxima. They are neither line coverage nor exhaustive proof. Requested controls that fail to reach their trigger are distinguished from activated controls in the plan campaign.
