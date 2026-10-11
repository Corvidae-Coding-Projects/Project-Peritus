# Independent role-policy source review 02

Reviewer: `/root/sol_runtime_phase`

## Verdict

PASS for the bounded production-correspondence claims in the supplied checkpoint. I found no source-level mismatch, incomplete field contract, changed public admission behavior, executable proof precondition, alternate cfg implementation, or unsupported claim within the reviewed scope.

This is an independent source review plus validation of the supplied source/log identities. I did not rerun Cargo or Verus. It does not discharge context graph admission, selection, rendering, compaction, persistence, observed-content authenticity, or caller-wide enforcement.

## Source identity

- Frozen root: `/tmp/peritus-parent-role-policy-source-02`
- Full 17-file manifest was checked successfully; manifest SHA256 `af21cf03dfbc25f44e373ac08bbddbe217b5bff271528af69c2b3bd61ed7f385`.
- Changed 12-file manifest SHA256 `d868078e87abe1288246879ca79f7c9ec81aa1196c3691946717aa1a79f31d0d`.
- Patch SHA256 `f874fb0736ebe20c9abf7befcdf2c70c812d3e2249abf500f875520317f9c982`.
- The isolated workspace's complete role package matches the frozen 17-file manifest.
- Its 21-file `peritus-spec` dependency matches `/tmp/peritus-parent-role-policy-spec-dependency.sha256`; the manifest SHA256 is `207f982395f69bf78f294ea773d9dfcece4470592f24bb267be48e148c784700`.

## Source findings

- `src/harness_role.rs:23-73` exhaustively maps all five harness roles to the intended `ActorRole` values and gives the exact inverse over all eleven `ActorRole` variants. The remaining six roles return `None`.
- `src/context_class.rs:41-80` gives distinct exhaustive ranks for all fourteen classes. `ContextClassSet::new` at lines 123-164 checks empty first, then the first adjacent pair in input order, classifies equal-rank pairs as duplicates and descending ranks as noncanonical, and retains the rejected current class. The rank implementation and spec are identical; unique exhaustive ranks make rank equality equivalent to enum equality.
- `src/error.rs:30-87` exposes and preserves all three stored fields. Plain, class, and operation constructors require the other optional payloads to be absent.
- The executable context tables in `src/context_policy/tables.rs` are field-for-field equivalent to the preimage implementation. `src/context_policy/model.rs` and `src/context_policy.rs:65-80` bind exact visible, contributable, required, freshness, memory, reasoning, ancestry, and presentation values for all eleven roles. Writer/Fixer/Evolution, Reviewer, Evaluator, GateRunner, and the remaining restricted roles each match their actual branch.
- `ContextClassSet::from_canonical` is total and promises only exact storage. Its complete caller set is private to `context_policy/tables.rs`; each production policy constructor proves the exact finite table via `spec_for_role`. No type invariant or arbitrary-input canonicality is claimed.
- `src/capability_view.rs:20-75` defines complete nonempty, permission, adjacent-order, and first-error predicates. The production constructor checks permission before duplicate/order at every position and carries a valid-prefix invariant, so the existential error witness is necessarily the first bad position. Empty input precedes all per-position checks. Success retains the exact role and supplied sequence.
- The operation rank spec and executable rank at `src/capability_view.rs:253-290` enumerate all fourteen `OperationClass` variants with unique equal values. `for_role` covers every `ActorRole`; each exact sequence is a narrow subset of the actual `ActorRole::permits_operation` table, including intentionally narrower Orchestrator, HumanAuthority, DaemonService, and ProviderToolWorker views.
- The four semantic Clone implementations cover every stored field: ContextClassSet values; CapabilityView role and operations; ContextPolicy's three class sequences, four policy values, and complete presentation value; RoleProfile actor role, harness role, context, and capabilities.
- `ReviewIndependenceView::from_contract` uses the six actual contracted getters, whose dependency postconditions bind them to all six `ReviewerIndependence` fields, and independently fixes fresh context to true. `PresentationProfile::new` binds the requested style and all three enabled facts.
- `capability_view_is_narrow` and `reviewer_context_is_fresh` call the production getters and expose exact predicates. The latter's deterministic-profile implication follows from the exact reviewer policy and does not claim freshness enforcement for arbitrary profiles.
- The only `cfg(verus_only)` items are imports and the ghost model module. There is no alternate executable implementation. The public executable APIs add ensures clauses only; the sole requires clause is on the private validated per-position helper and its caller proves the index bound.

## Supplied evidence checked

- Strict logs `/tmp/peritus-parent-role-policy-verus13.log` and `/tmp/peritus-parent-role-policy-verus14-restored.log` are byte-identical (SHA256 `6e18220c69278c96c405b3bc258d7b3795b37e4383f47b9b8cae3bf9777aa6cd`) and report 77 verified, 0 errors under the documented strict command.
- Negative log `/tmp/peritus-parent-role-policy-negative-hidden-reasoning2.log` (SHA256 `128010feac956f32efc4759c801f6159909da492d57d496cbd3aab85a2e3de2d`) reports the reviewer constructor postcondition failure after adding `HiddenReasoning`. The recorded restoration matches all 17 frozen files and the restored strict run passes 77/0.
- Supplied default and all-feature logs each report 12 passing tests; Clippy and fmt exit successfully. The API log reports 3,404 formal-boundary files and 14,629 ordinary-safe entry points; layout reports 4,290 source files.

## Precise limits

The finite production tables are proven equal to their finite ghost definitions. The checkpoint does not prove a general canonicality/subset theorem for arbitrary `ContextPolicy` values, nor does the total private storage helper create such a type invariant. The role crate constructs immutable policy data; downstream selection and enforcement remain separate obligations. The supplied Verus run emits automatic-trigger notes for the capability first-error existential, but reports no proof error.
