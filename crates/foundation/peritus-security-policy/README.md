# peritus-security-policy

## Folder guide

H0 security-readiness policy for exact Peritus release candidates.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-security-policy`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | H0 security-readiness policy for an exact integrated candidate. |

`peritus-security-policy` is the pure V-class H0 security-readiness reducer. It binds every
observation to one exact integrated release candidate and evaluates the authoritative
`R-SEC-001` through `R-SEC-007` requirements together with acceptance criteria 9–12, 17–19,
24, and 25.

The crate performs no I/O and grants no release authority. A `Ready` decision means only that the
supplied canonical evidence satisfies the H0 security policy. Release authorization remains owned
by the later H4 release transition.

## Invariants

- `SEC-INV-001 ExactCandidateBinding`: source, release artifacts, qualification plan, and the
  complete `RevisionTuple` match for every contributing observation.
- `SEC-INV-002 CompleteControls`: all seven security requirements and all nine in-scope numbered
  acceptance criteria have a passing observation.
- `SEC-INV-003 AccountedTrustBoundary`: the threat, control, unsafe-code, and TCB inventories are
  present and complete.
- `SEC-INV-004 IndependentReview`: the external review is completed by an actor and organization
  independent of the producer.
- `SEC-INV-005 FindingClosure`: no critical or high finding is open or merely accepted as risk.
- `SEC-INV-006 CompleteEvidence`: every required evidence-manifest role is present with a nonzero
  digest.
- `SEC-INV-007 FailClosed`: ordinary Rust callers cannot construct `SecurityDecision`; `Ready`
  requires an empty unmet-condition sequence and every verified phase check.

## Dependency policy

Production dependencies are restricted to `peritus-types` and `vstd`. Native execution,
orchestration, hashing, persistence, and external review collection belong to the C-class
`peritus-security-qualification` crate and its host adapters.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
