# GAP-03 cancellation selector and update independent review

## Verdict

**Bounded PASS.** I found no blocking functional, specification, compatibility, or proof-trust defect in the reviewed cancellation increment relative to base `a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8`.

This is an independent agent review, not human approval and not a claim that GAP-03 is complete. It covers the finite cancellation selector, ordered identity set, composed reservation-sensitive update loop, `WorkSpec` parent accessor, production `control::cancel` integration, exact reservation removal contract, cancellation regressions, and the mechanical source/test/checker cleanups required by the final repository gates.

## Findings

### Reachability is exact, finite, and nonvacuous

`reachable(work, root, id)` is the intersection of every parent-closed identity set containing `root`, which defines the least parent closure. The definition is nonvacuous: the universal identity set is a model, and the executable fixed point produces a concrete root-containing parent-closed set. `root_reachable` and `child_reachable` establish soundness; a no-change pass establishes parent closure of the computed set and therefore completeness through the universal definition.

The loop terminates without a tree or acyclicity precondition. Its variant is the finite set of retained identities plus `root` that has not yet been inserted. Every changed pass inserts at least one previously absent identity, and ordered insertion prevents duplicates. A reached parent cycle terminates and selects its connected component; a disconnected cycle stays unselected. Missing parents are total and deterministic. Ordinary admission cannot create a parent cycle because a parent must already be retained, but the selector proof does not rely on that stronger state property.

Traversal uses every retained record without consulting its lifecycle phase. Phase filtering occurs only after closure. A terminal ancestor can therefore carry reachability to a live descendant while the terminal ancestor itself is excluded from `affected`, matching the required behavior.

### Selection is complete and canonical on valid state

`affected` is exactly the pre-state work sequence filtered by `selected` and mapped to work IDs. It neither invents identities nor omits a selected record. Its origin-index proof establishes that a canonically ordered valid work vector yields a strictly ordered, duplicate-free result. That order matches the sorted identity order emitted by the base implementation on valid state. Root-only cancellation selects only the live root; tree cancellation selects the live root and every live reachable descendant.

The binary-search identity helpers have private ordering requirements, but no caller-visible precondition. `closure` establishes and preserves orderedness before every lookup and insertion. `affected`, `cancel_retained`, and the production control wrapper expose no proof precondition. `WorkSpec::parent` preserves the existing public `const fn` behavior and now has the exact `result == spec_parent` contract.

### The update trace matches the executable loop

`cancel_retained` computes the complete affected vector before mutating state and applies it in that order. Every step consults the unchanged pre-state reservation sequence. A selected identity with an active reservation moves to `Cancelling`; a selected identity without one is terminalized with exactly `WorkTerminal::Cancelled`.

`cancellation_updates_match` is an ordered, nonvacuous sequence of concrete work states with one checked transition per requested identity. On a valid state, the selector's no-duplicate result means each target is updated exactly once. `unselected_work_unchanged` fixes every unselected record by direct equality. The composed relation preserves all scheduler fields outside the work vector and preserves the reservation invariant whenever it held initially.

The simplified `cancellation_result_matches` is equivalent to the earlier existential contract. For success, the prior witness was forced to `affected.len()`, which is the direct full trace now required. For failure, `0 <= processed <= len` together with `processed < len` is exactly `0 <= processed < len`; the exact successful prefix, first missing target, and existence of every earlier target remain required.

`apply_updates` proves `complete == all_targets_exist(old work, affected)`. Because `affected` proves that every emitted ID occurs in the pre-state work vector and lifecycle mutation preserves the ID layout, `cancel_retained` proves completion without a hidden precondition. The private `control::cancel` missing-target branch is unreachable on this path. Unknown and terminal roots still reject before mutation, and the event contains the exact pre-state selected vector.

The final module split preserved executable `is_active`, `update_one`, and `apply_updates` plus the public specifications in `update.rs`. `frame.rs` and `trace.rs` contain proof-only helpers with `pub(super)` visibility and explicit imports. Their `requires` clauses are discharged inside the reviewed composition. The work-mutation split likewise keeps the runtime search in `work_update.rs` and moves the exact indexed mutation into private `apply_at.rs`; its index/identity requirements are established by the search caller.

The composition uses `hide` only to prevent repeated SMT expansion of already checked existential definitions. It does not assume their results. I found no `external_body`, `assume`, `admit`, axiom, or equivalent proof bypass in the reviewed cancellation/update source.

### Active ownership and exact removal remain intact

Cancellation retains reservations and resource ownership for selected work in `Reserved`, `Running`, or already `Cancelling`, while moving it to `Cancelling`. Inactive selected work is terminalized immediately. Pending directives continue to request cancellation for retained active dispatches; late success is rejected; acknowledgement releases ownership and terminalizes the work.

The strengthened `remove_reservation` contract ties `Some(reservation)` to the exact old sequence element and the exact old sequence with that index removed. `None` requires exact sequence equality and absence of the requested dispatch ID. Its frame fixes every other `SchedulerState` field, and it conditionally preserves reservation ordering and the reservation invariant. The executable removal algorithm is unchanged.

### Final cleanup did not weaken policy or behavior

The ordinary-API checker now recognizes only the exact token sequence `cfg_attr(verus_keep_ghost, verifier::verify)`. Wrong conditions, unqualified markers, trailing or additional attributes, and `verifier::external_body` remain rejected. A hostile regression confirms the marker cannot conceal a public executable `requires` clause. This marker opts an ordinary Rust declaration into the pinned Verus verifier; it does not trust or externalize a body. The final strict proof run checks the declarations using it.

Test fixture loading changed from embedded-byte macros to `std::fs::read` under the audited `CARGO_MANIFEST_DIR` root. Fixture paths and fixture bytes are unchanged. Renaming the private verified fence classifier from `admit` to `classify` avoids a false lexical trust match; its implementation and postcondition are unchanged. The last three lint fixes change helper visibility only inside private integration-test modules, so they do not alter library APIs or runtime behavior.

## Regression coverage

The integration tests use an independent parent-chain oracle. They cover reverse identity order requiring multiple closure passes, live descendants behind a terminal ancestor, an unrelated branch, root-only cancellation, unknown and terminal rejection with unchanged input, and active descendants in `Reserved`, `Running`, and `Cancelling`. The active trace checks retained reservations/resources, pending cancellation directives, late-success rejection, acknowledgement release, and exact replay/checkpoint round trips.

Private unit tests exercise arbitrary finite graphs that admission cannot normally construct: a reached parent cycle, a disconnected cycle, missing parents with retained and missing roots, terminal-ancestor traversal, and shuffled input preserving the exact selected subsequence. These cases support the proved absence of an acyclicity, canonical-input, or root-existence requirement in the total selector.

The base implementation already performed repeated complete work scans until closure, binary membership checks, per-insertion sorting, per-target reservation scans, and per-target work scans. The verified implementation retains bounded whole-scan behavior while replacing repeated complete sorts with ordered insertion. A reverse-topological chain can still require one discovery per pass, so selector identity comparisons and the later per-target scans have quadratic worst-case structure at the retained-work bound. This is an inherited, unmeasured performance concern, not a new regression or an experimentally demonstrated denial of service. This verdict makes no linear-time complexity claim.

## Frozen source identity

The final snapshot is `target/gap3-evidence/140-final-checkpoint-source.sha256`, SHA-256 `6e2513f45819de038fdffb8775eafdff76e5152efd258023578ae7f92f011960`. It contains 1,196 entries. I independently ran `sha256sum --check --quiet` against the live worktree and observed exit 0.

| Reviewed path | SHA-256 |
|---|---|
| `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation.rs` | `d46a5056b5eeffd4274a6c7db2924096b08e63cf457ca42c320ba88670c57bf8` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/ids.rs` | `6210b94e9926b3bcda5cfd4a20d8e64bd417c5e42203558ef04e221ec7d995ef` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/update.rs` | `a502848d29e88df5a45839a22ab36ae6fd07cd5b4226f515339a8bc88e2fcd50` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/update/frame.rs` | `c899a186e1deb25a71f2696ceab5ad612866a4cda9462f470754e8b85ed2ec22` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/update/trace.rs` | `53a7f3d940fcd9e34aea558dda6f4f2274d46e4b9512cdc47da203c0443e5bc8` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/tests.rs` | `8bcb21405b22d65075ab06db0d831270bda0796201005eebb42e8425d2389deb` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/control.rs` | `d179fd61d9a206d389d21bd3bf68f5003cb0bf48b7b48a8ea1c74c8a086aed7d` |
| `crates/orchestration/peritus-scheduler/src/work.rs` | `31f8fbba5ab258bdb44007ea79f621a448af5c658468e061b0f409d42339b200` |
| `crates/orchestration/peritus-scheduler/src/state/mutation/reservation_remove.rs` | `77bf048d6b509a8569b92afc371e0d9452d5d6aa24f251cf0fbed3de65a721e2` |
| `crates/orchestration/peritus-scheduler/src/state/mutation/work_update.rs` | `dc834ba94c955dbff6416e74a0551af31b8742b8964aeaa2fe1f5cd765e69c93` |
| `crates/orchestration/peritus-scheduler/src/state/mutation/work_update/apply_at.rs` | `38e4d3f73370318d9565c5462ced6775beb12124ca9ae19891b7411e79080584` |
| `crates/orchestration/peritus-scheduler/tests/cancellation_tree.rs` | `06aaee85e58b26dffe16c295c91e1d9922b63402e4f823255bdf70fdb13476b8` |
| `crates/orchestration/peritus-scheduler/tests/cancellation_tree/oracle.rs` | `fd7c51fd884441a5c0575fad84be743c271bc2659b7ff351010d3da305610da6` |
| `xtask/src/api_contract/expansion/attributes.rs` | `122338fdf6aff5a5c80fe100eff1f70c375f9e6852f64404743d16eafc59a716` |
| `xtask/src/api_contract/expansion_tests.rs` | `5ae192a5818c5756bdd83ee6fd17402a9f55baaade2bdc13afde128d14e295b8` |

The complete manifest also binds the daemon fixture readers, fence rename, split protocol/event modules, the other two integration-test visibility cleanups, fixtures, generated protocol artifacts, and design inputs reviewed by the wider GAP-03 effort.

## Qualification evidence

| Evidence | Observed result | SHA-256 |
|---|---|---|
| `128-final-checkpoint-verus.log` | strict Verus, `--no-cheating --rlimit 20`: 498 verified, 0 errors | `c9e5baa1217b1d265167015010bc359fa623f47474bba18a56b0dc24559c3c0c` |
| `141-final-checkpoint-tests.log` | scheduler/protocol/projection: 109 passed, 0 failed, 0 ignored | `7d8341e656e148154a5db6740e9253abe46cfe3b64c4ec019e71ac876634ee9c` |
| `125b-final-daemon-tests.log` | daemon library: 178 passed, 0 failed, 3 existing ignored | `e3aff7e1209ff71adfef1ed9cb2346ded348fc20f3d525fdf2a5a2b78bd59783` |
| `126-final-api-checker-tests.log` | 45 API-contract tests passed, including exact marker and hostile forms | `41dea25180aa00a209a4df51741f45fbea95579f9334eed8740cb27a19285487` |
| `127b-final-checkpoint-clippy.log` | strict Clippy passed for the eight changed packages | `bcad1b58cedf85b5122d62717e1727bc63d9e4d71d1209338108a34556a21915` |
| `142-final-checkpoint-clippy.log` | final eight-package Clippy rerun after test-only formatting passed | `c46f006df1f7644a031a3a463cbfce79ef9b0c2beccc96a700289b0741a15f53` |
| `130-final-ordinary-api.log` | passed: 3,601 formal-boundary files, 14,755 ordinary-safe executable entry points | `b97dc00d319479610baae03e19606b4dabc780678760f1d46bbdc91fe2061857` |
| `139-final-format-check.log` | formatting passed | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `134-final-architecture.log` | passed: 84 packages, 4,514 source files | `6bda7e33aed4f116c0aefef57efcf75e6cc961fbb8cd07c89e259bc483d8b319` |
| `135-final-reproducibility.log` | passed: 141 immutable action references | `3d3a97824c3559c75591afecbba6724d7a231a13fcdbfae1af59f07a0896cc24` |
| `137-final-docs-check.log` | passed: 249 documentation files | `54438d70002e8071cfb5da0a44c60d254178a049f53addd68a73009c71663cfd` |

The strict Verus run preceded only a formatting-only change in an integration-test oracle; every production and verifier input remained byte-identical. The final test and scheduler Clippy runs use the formatted bytes bound by snapshot 140.

`133-final-protected-trust.log` is **not green**. It exits with 87 proof-impact inventory violations: exactly 49 changed files have stale reviewed fingerprints and 38 new files lack fingerprints. Inspection found no trusted-construct violation in that log. Its SHA-256 is `27c962c7cdbe51a9450aa58bfcf5b778b87eddd545bd3d5785ba47d3e3a65e63`. This is an explicit source/trust-inventory reconciliation gap and prevents a full delivery or whole-GAP-03 pass claim.

## Remaining boundary

The checked contracts prove the cancellation closure, target projection, ordered reservation-sensitive work updates, unrelated-state frame, and exact reservation removal. The integration suite exercises the public cancellation event, acknowledgement/release, replay, and checkpoint behavior, but this review does not claim one formal whole-`decide` theorem covering event/cursor/digest construction, every rejection path, replay equivalence, worker-loss interaction, and every reducer invariant across all variants.

The unresolved protected-trust fingerprints and the broader whole-reducer/release composition remain outside this bounded verdict. They do not invalidate the proved and tested cancellation behavior described above, but they do prevent claiming GAP-03 as fully qualified from this report alone.
