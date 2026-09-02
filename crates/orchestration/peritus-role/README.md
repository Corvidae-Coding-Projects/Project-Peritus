# peritus-role

`peritus-role` projects the stable B1 security roles into deterministic C6 context policies and
read-only capability views. It does not define security identities, issue capabilities, evaluate
authority, or perform effects.

The writer, reviewer, fixer, evaluator, and evolution-agent profiles are explicit. Other B1 roles
receive restricted profiles. Each profile defines canonical visible, required, and contributable
context-class sets; memory and hidden-reasoning visibility; fresh-context and producer-ancestry
rules; and provider-neutral presentation policy.

Reviewer context is always fresh, excludes producer ancestry, memory, and hidden reasoning, and
exposes inspection only. Writer and fixer views exclude acceptance, waiver, policy amendment, and
harness promotion. Evaluator and reviewer views exclude mutation. The public checked
`CapabilityView` constructor rejects any operation denied by the underlying B1 role, and its
executable `is_narrow` result is proved equivalent to the formal B1 subset predicate.

`ReviewIndependenceView` copies every immutable B2 reviewer-independence requirement and adds the
C6 fresh-context requirement. It requests evidence from the future review engine; it never claims
that evidence already exists.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-role
```
