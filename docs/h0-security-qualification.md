# H0 security qualification

H0 is a release-candidate security qualification, not a component self-test and not release
authority. Its subject is one exact integrated candidate: the complete `RevisionTuple`, source-tree
digest, release-manifest digest, and immutable qualification-plan digest. A change to any component
creates a different subject and invalidates prior case, inventory, review, finding, and manifest
evidence.

The V-class `peritus-security-policy` crate owns the deterministic Ready/NotReady policy. The
C-class `peritus-security-qualification` crate owns the effect boundary, closed catalog,
fresh-subject orchestration, resource and cleanup accounting, canonical evidence manifest, and
bridge into the verified reducer. Neither crate performs H4 release authorization.

## Authoritative scope

The policy uses the architecture identifiers without semantic renaming:

- `R-SEC-001`: the model is untrusted and cannot grant authority or mutate authoritative state;
- `R-SEC-002`: effects require scoped, expiring, actor-bound capabilities checked at execution;
- `R-SEC-003`: handle-resolved path policy resists traversal, symlink races, mounts, case folding,
  device names, and platform aliases;
- `R-SEC-004`: native backends enforce filesystem, process, network, environment, secret, and
  resource controls;
- `R-SEC-005`: Git, Peritus, policy, trust, evaluator, secret, and approval metadata is protected;
- `R-SEC-006`: provenance is typed and cannot silently change authority precedence; and
- `R-SEC-007`: dependencies, plugins, artifacts, SBOMs, provenance, licenses, and signatures are
  auditable and reproducible.

H0 also evaluates acceptance criteria 9–12, 17–19, 24, and 25 exactly as numbered in the production
architecture. Requirement and criterion observations are independent aggregates: every probe maps
to one literal R-SEC identifier and one literal criterion, and both closed sets must pass.

## Production campaign

The immutable 42-case catalog emphasizes runnable failure boundaries:

- malicious repository traversal, symlink race, submodule/worktree escape, case aliases, Unix and
  Windows device paths, shell injection, poisoned instructions, oversized output, terminal escape,
  and seeded-secret exfiltration;
- native Linux, macOS, and Windows sandbox capability contracts, escape attempts, and default-deny
  network behavior;
- plugin and MCP capability scope;
- reviewer read-only, fixer non-approval, and writer non-waiver role isolation;
- exact candidate mutation invalidation;
- sealed-answer, evaluator, profile, self-promotion, and evolution-campaign isolation;
- immutable promotion-gate binding and atomic rollback history;
- source citation, infrastructure/task failure taxonomy, and secret redaction;
- locked dependency reproducibility, signatures, SBOM/provenance, licenses, and migration/recovery
  documentation;
- unsafe-code, TCB, threat, and control inventory reconciliation; and
- finding lifecycle, bounded cancellation, process-tree termination, and complete cleanup.

The catalog is represented in Rust for type-safe execution and in `security/control-catalog-v1.toml`
for review. `security/threat-model-v1.toml` records assets and threats. The unsafe and TCB files are
reconciliation contracts, not pre-filled passing results; their probes must compare them with the
exact candidate.

## Native runner contract

`QualificationRunner` executes in stable probe order. `FreshSubjectFactory` must provision a
never-before-used disposable subject for exactly one case, including target-specific Linux, macOS,
and Windows hosts where required. `QualificationSubject::execute` receives the exact candidate,
case, cooperative cancellation signal, and hard limits for monotonic duration, processes, peak
memory, output, and artifacts.

The production runner has a fixed three-host partition. The Linux shard executes the portable
tier-one catalog and the Linux backend probe; macOS and Windows execute only their corresponding
native backend probes. The aggregator requires one shard from every platform, identical candidate
and limit bindings, globally unique fresh subjects, and exactly one report for every canonical
probe. It then restores the original 42-case order. Missing or duplicate platform evidence cannot
be interpreted as portable success.

The adapter returns a `NativeExecutionReceipt` containing executor, host, and exact-command
digests, exit status, native-sandbox observation, resource accounting, and nonempty structured
evidence. There is no simulated or assumed-success receipt variant. Host adapters are nevertheless a
trust boundary: H0's independent external review must inspect their implementation and native
evidence.

Cleanup is attempted exactly once after every successful provision, including assertion failure,
adapter error, cancellation, and panic. Cleanup accounts for remaining processes, paths, mounts,
and endpoints and supplies a direct evidence digest. Reused subjects, missing cleanup, subject
mismatch, or any remaining resource fails the case. Cancellation and limit overrun never become a
pass.

Some Unix workspace and overlay filesystems can briefly return `ETXTBSY` immediately after a fresh
executor is staged. The native boundary retries only that exact operating-system condition four
times with a short bounded delay. Any persistent busy state or different launch error remains a
typed native failure.

## External review and findings

External review is supplied separately as `IndependentSecurityReview`; the runner and report
reducer never create it. The reviewer actor and organization must both differ from the producer,
the review must be complete, and its report and fresh context are digest-bound. Findings are sorted
by stable `FindingId` and bound to the same exact candidate. The review must explicitly cover native
sandbox escape, authority isolation, evolution/promotion, supply chain, and unsafe/TCB scope.

Critical and high findings block readiness while open or merely accepted as risk. They clear only
after both remediation and independent-retest digests are present for the exact candidate. Missing,
incomplete, stale, or non-independent review is NotReady.

## Canonical evidence

Each case retains only bounded structured facts, counts, safe taxonomy codes, and digests of raw
artifacts held elsewhere. This prevents arbitrary terminal, prompt, shell, or secret-bearing text
from entering the default manifest. Entries are uniquely labelled and ordered bytewise.

`EvidenceManifest` serializes a fixed-field JSON structure in stable case and finding order and
computes SHA-256 over those exact bytes. Its schema is
`security/schemas/evidence-manifest-v1.schema.json`; the external review and control catalog have
separate versioned schemas. Required policy artifact roles include the plan, native results,
resource accounting, cleanup ledger, threat/control and unsafe/TCB inventories, external report,
finding register, supply-chain attestation, and exact release manifest.

## Readiness semantics

The verified policy returns `Ready` only when all of these facts hold together:

1. every contributing observation binds the exact integrated candidate;
2. all seven R-SEC requirements pass with evidence;
3. all nine in-scope numbered criteria pass with evidence;
4. threat, control, unsafe, and TCB inventories are complete;
5. an independent external review is complete;
6. no critical or high finding lacks remediation and independent retest; and
7. all required canonical evidence roles have nonzero digests.

`SecurityDecision` has private construction, and its ordinary `is_ready` wrapper refines the same
phase conjunction and empty unmet-condition sequence used by Verus. `QualificationReport` adds the
stronger operational condition that all 42 native cases passed with cleanup. A Ready report may be
consumed as H0 evidence by H4; it cannot itself sign, tag, publish, promote, or release anything.

## Host integration

Application integration must provide native adapters, exact candidate identities, artifact-store
retention, and an independently obtained review record. CI/release integration must additionally:

`NativeProbeFactory` is the standard fresh-process adapter. It stages one reviewed executor into a
new private root for each probe, sends the versioned candidate-bound request, owns the complete
process tree, validates the bounded response, and removes the root before returning cleanup. It
also assigns a retained-artifact root named by the fresh subject ID and verifies every digest entry
against the named regular file's actual length and SHA-256. Linux and macOS use a dedicated process
group; Windows uses a kill-on-close Job Object. Platform probe executables still own the actual
assertions and must write their raw evidence under that assigned root. A structurally valid response
cannot replace those effects or the independent review.

1. validate the checked-in schemas and reconcile the Rust and TOML catalogs;
2. run platform-specific cases on native tier-one hosts rather than cross-compiled binaries;
3. retain raw artifacts by the digests named in canonical JSON;
4. publish both canonical manifest bytes and their SHA-256;
5. require the V-class verification target before accepting the H0 report; and
6. pass Ready evidence to the separate H4 release-authority transition.
