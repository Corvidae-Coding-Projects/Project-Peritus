# peritus-policy

`peritus-policy` is the verified, effect-free authority core for Peritus. It evaluates immutable
policy definitions, enforces compiled role separation, preserves exact resource/capability pairs,
tracks monotonic authority time, and produces move-only logical capability transitions.

## Invariants

- Policy evaluation is total for every checked policy and request.
- Permission sets contain exact `(ResourceId, CapabilityName)` pairs and are canonical.
- A matching immutable or restriction denial dominates grants and approval requirements.
- A restriction layer is neutral when no rule matches and can never grant authority.
- Reviewer/evaluator roles cannot mutate workspaces; writer/fixer roles cannot accept, waive,
  amend policy, or promote harnesses.
- Capabilities remain bound to one actor, role, environment, revision tuple, validity window, and
  exact permission set. Limited use counts decrease exactly once per accepted logical transition.
- Authority-clock observations never regress or cross epochs implicitly.
- An amendment preview retains the original ceiling and immutable denials and changes exactly one
  declared restriction tier. It is not an active-policy fact.

## Boundary

This crate performs no hashing, serialization, storage, clock I/O, target resolution, tool
dispatch, or effect execution. Transition digests and action digests are exact correlation bytes,
not authenticity claims. C0 and later target gateways own durable commit and effect permits.

The production dependency set is limited to `peritus-types` and the workspace-pinned `vstd`.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-policy
```
