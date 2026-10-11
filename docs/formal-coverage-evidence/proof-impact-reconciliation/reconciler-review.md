# GAP-02 audit exporter and reconciler review

Date: 2026-09-13

Repository: `/home/doll/Project-Peritus/.worktrees/formal-coverage`

Agent task: `/root/impact_evidence_audit`

Assigned runtime profile supplied by the parent: `gpt-5.6-sol`, reasoning effort `xhigh`.

Mode: independent read-only repository review. I made no repository, Git-ref, build, network-setting, approval, actor, or verification-register change. This report is the only file I wrote. The task/model/effort metadata is not canonical actor provenance, a PCR review identity, or an approval.

## Verdict

**Pass with explicit operational limits for the final frozen source.** The exporter and reconciler now produce deterministic source identities and direct/shared affected-package sets for the supplied exact Git commits, preserve approval/provenance bytes exactly, and keep every candidate obligation truthfully pending. They do not create or imply a PCR approval, gate pass, actor enrollment, or obligation discharge.

The initial review found seven material boundary problems. The parent repaired each before the final freeze: non-raw proof-impact comparison, annotated-tag misidentification, `git archive` byte transformation, unbounded materialization, unsafe/partial output publication, missing hashes for non-formal obligation evidence paths, and mutable/under-validated exporter execution. No blocking finding remains in the final `reconcile.py` bytes identified below.

## Frozen repository identities inspected

| Role | Commit | Tree | Local object result |
|---|---|---|---|
| Supplied protected base | `8e8cbb1bcf0de9787d86c7be3d6fc1f2079f3493` | `3e5febb6c672bf24e86a1c8823f2e931fb33aef1` | Exact commit object; ancestor of candidate |
| Supplied candidate | `d0e4f0cf0943c7194adaaeabcb11fc70badc3fe8` | `73791a6d6079fd026cf6595fdc37dcf565456212` | Exact commit object |

The script correctly treats those identities as inputs. It cannot establish that the first commit is still the protected `develop` tip or that the second is still PR #74's head; that external branch/hosting fact remains the caller's responsibility and must be refreshed before authorization.

## Final reviewed source hashes

SHA-256 over the exact working-tree bytes reviewed after the final repair freeze:

```text
3f1a8f4105781475721cff2fdd4c0dad3150a5ed35b85c770f8e604495b84dc0  docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile.py
032a7cc4dc9c8b43b5f3f36250f8b298b33dc6564ee00635dde627aa41edf230  docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile_tests.py
84878f82e7cb8311dcb29aeb3121213c7e2745167237efcf561e54cc55ab219b  xtask/src/trust/manifest_impact/snapshot.rs
9b8379591a22cb51d61e51d6c534fcb746b254816e62862c84518ee97b0509f9  xtask/src/cli.rs
a131001cf60f52e4a106cde537af6e73aecfd1b7eccfb674d64832a542aeb58a  xtask/src/cli/help.rs
ef8eb1cd0704573fe4659d39d9d5f800993f188d2e1f5d9802d4fcd782726267  xtask/src/trust.rs
461f12cb2cc1cfc9893813e3e1345d85812dd2c94a8156fc8c05b8e2d668a036  xtask/src/trust/manifest_impact.rs
9208b82787a382d5534e1df4ce3962e5c68cabffd726bd6bb84e44a8c3f80b3c  xtask/tests/cli.rs
9f7eee9d76b743fd1b9323d03745384e6620245bdad5a1ad364e089705cc02f3  xtask/README.md
```

Reviewed unchanged implementation dependencies:

```text
95d434de640b2d41e2e2d677f72e4e939f25014d053dce3afad8b21fad68b7d7  xtask/src/trust/manifest_impact/inventory.rs
6d0e18169f36efd56cf14b6e5f85494bd2a4ffc1dc6c02513dbc3219f61fbfae  xtask/src/metadata.rs
53efc44a36b5d24646bf4a0d1ff1396c07f7db41120df46a7e50d7af75ac0f65  xtask/src/source.rs
282a6891872f2e6c64bf9de52adc215f491cc8a6f0861c6bd6ea16d3c62b1c8f  xtask/src/source/trust_discovery.rs
596d257f28b048fb79c7f632334683d7192ac12647d74558d7647e6ff7712596  xtask/src/trust/manifest_impact/candidate_inventory.rs
95153ffc9478e42a030f0ae4d8e9939b3311851be05f870ab9a0df57d18856a8  xtask/src/trust/manifest_impact/candidate_tree.rs
093c1570542c70a65da96fcde4bc8d80ed3181a7606722f577eb10d217aa4b23  xtask/src/trust/manifest_impact/authorization.rs
f91d3ce3c38afeaa6dbe2146ec36bbc18f642e2ad5500b6155cbcb03fb0ce871  xtask/src/trust/manifest_impact/review_base.rs
e519c777c81212b0cd911b12df13eb8998aa875310deb8019576602588d48f95  xtask/src/trust/manifest_impact/checker_binding.rs
```

## Exporter result

`proof-impact-inventory` is a narrow audit exporter. It reuses the trust gate's `workspace_target_policy`, compilation-source discovery, and `inventory::expected_sources`, and hashes raw bytes with the existing `sha256_hex`. `BTreeMap` source ordering and sorted package construction give stable JSON ordering. Discovery diagnostics fail the command. The output has an exact envelope:

```text
schema          peritus.proof-impact-inventory
schema_version  1
status          audit-only
hash_algorithm  sha256-raw-bytes-v1
```

It emits no `changes`, no `change_id`, no actor, no verdict, and no authorization field. Its limits explicitly deny test/proof/review/protected-base/discharge claims. The CLI test checks an independently recomputed `Cargo.lock` digest and the absence of approval fields. This command writes only its caller-provided output stream in the xtask code; it invokes `cargo metadata --locked --no-deps`, so “audit-only” is an authority classification rather than a guarantee that Cargo never touches its host cache.

For both supplied commits, every emitted source SHA-256 matched the exact Git blob bytes. Source keys were canonically ordered; every affected-package list was sorted and unique. An independent architecture-root check found zero affected-package mismatches and zero sources outside a formal package root across 3,432 protected and 3,640 candidate inputs. Both inventories contain the eleven shared inputs with all 66 current formal packages: 53 H, 12 V, and one T.

## Reconciler boundary result

The final script now has the following properties:

- It requires lowercase full 40-hex identities whose object type is directly `commit`, rejects tag objects, requires distinct commits, and verifies base ancestry.
- It enumerates the complete tree with `git ls-tree -r -z`, rejects symlinks, submodules, duplicate/non-normal paths, and more than 30,000 entries.
- It streams raw blobs with `git cat-file --batch`, caps aggregate declared bytes at 512 MiB, recomputes each Git SHA-1 blob identity, and computes the retained raw-byte SHA-256. This avoids checkout filters and `export-subst` transformation.
- It freezes the requested exporter executable into a private temporary directory before either tree is read, checks source/copy/source hashes, executes only the private copy, and records the copy's exact hash.
- It validates the complete inventory envelope before trusting the result and checks every inventory digest against materialized Git bytes.
- It hashes the proof-impact manifest as raw bytes and requires its raw hash, parsed content, every retained review artifact, `actors.toml`, and `actor-provenance.json` to match across base and candidate.
- It computes obligation source/evidence hashes from the exact candidate Git object even when a referenced test is intentionally outside the formal impact inventory. Eight such evidence paths are explicitly listed rather than omitted or misclassified.
- It creates artifacts in a private temporary directory and publishes them only to a caller-selected path that must not already exist, including as a symlink. Validation failures publish no result directory. An operating-system copy failure can leave a partial destination, but the command fails and prints no success summary.

The Python program is therefore **artifact-producing, not filesystem read-only**. Its persistent writes are limited to the fresh output path explicitly supplied by the caller; tree materialization and pre-publication artifacts are disposable. It does not alter a checkout, Git object/ref/config, `verification/proof-impact.toml`, actor data, reviews, or obligation state.

## Exact retained proposal result

The retained files reviewed are:

```text
bfceaf4832bbeb22bcc16ef5b877d6c18b5d552999d39871c28504a71113c1f1  protected-inputs.json
a3ea7d6c77d35d0d278b7a1a6678f47d8f2a08585f25baeb71ad8b60a1492cc7  candidate-inputs.json
c7d535ba4c81be25d9c52435db05b7c52cc74fb2951a63566cf0cca3d3aa6831  reconciliation.json
```

The retained reconciliation records exporter binary SHA-256 `2098142e8d85d77813fc972c39c38e6c8bab102fc17740aee26d5ca3de47a857`, the exact frozen executable used for that generation. Two independent final runs produced byte-identical retained artifacts. An earlier pre-rebuild generation used binary `6a495710f7ecdf7f9327714fc34727011c6bf00e130a4c15a8f2ca3ea67a6f16` and reconciliation `af605182ada97cb98fe4cbe1486182663ec6acfb0aa9eefc7a8219011f17cecb`; comparison with the final result found exactly one differing top-level value, `inventory_tool_binary_sha256`. Both source inventories and every substantive reconciliation field were byte/value-identical. The documentation accurately calls this binary digest build-specific.

The retained proposal says, and the underlying Git/TOML evidence independently confirms:

| Field | Exact result |
|---|---:|
| Protected declared source rows | 3,390 |
| Protected actual inputs | 3,432 |
| Inherited declared-to-base transitions | 218 |
| Candidate actual inputs | 3,640 |
| Base-to-candidate transitions | 406 |
| Declared-to-candidate proposed transitions | 620 |
| Affected packages | 66 |
| Required gates | 132 |
| Candidate obligations | 153 `in-progress` |
| Final candidate discharge reviews | 0 |
| Removed obligation IDs | 0 |

The exact proof-impact SHA-256 is `39135a20188b9415102c51c04e44f53fad27ea73a0775bbb4623a251b88a6d44` at both commits. The 142 retained approval/actor records (140 review files plus the two actor/provenance records) have identical path/digest maps across both trees and independently matched raw Git blobs. All five existing PCR rows are `approved`; the proposal does not extend those approvals to the candidate.

The obligation section preserves all 153 declared candidate statuses as `in-progress`, sets `final_candidate_discharge_review` to JSON `null` for every row, and states that locators and patch reviews are not final-candidate discharge authorization. There are 137 pre-existing obligations and 16 candidate additions; 20 declarations changed. No output field says that a required gate passed.

## Review findings and disposition

| ID | Initial finding | Final disposition |
|---|---|---|
| R-01 | Parsed TOML equality allowed raw proof-impact comments/formatting to change while claiming history preserved. | Fixed: exact `proof_impact_sha256` equality is mandatory in addition to parsed equality. Current raw hashes match. |
| R-02 | `^{commit}` accepted an annotated tag object while recording the tag OID as a commit. | Fixed: `cat-file -t` must return exactly `commit`; the repository's full annotated-tag OID probe is rejected. |
| R-03 | `git archive` could apply `export-subst`, so extracted review/history bytes were not necessarily Git blob bytes. | Fixed: bounded raw `cat-file --batch` materialization plus Git blob identity verification. |
| R-04 | Materialization buffered an unbounded archive and had no file/byte caps. | Fixed: 30,000-file and 512-MiB aggregate bounds with 64-KiB streaming reads. |
| R-05 | Existing/symlink output names could be overwritten and failures could publish misleading partial validation artifacts. | Fixed: private generation plus exclusive fresh-path publication; validation failures publish nothing. Copy I/O failure remains visible as a failed command. |
| R-06 | Obligation evidence outside the formal inventory caused lookup failure and was not separately classified. | Fixed: exact candidate Git hashes and explicit `evidence_paths_outside_formal_impact_inventory`. |
| R-07 | The executable could change between base and candidate runs, and only `status` was checked in its envelope. | Fixed: private executable snapshot with hash checks, plus exact schema/version/status/hash-algorithm validation. |

No unresolved approval-fabrication, path-traversal, symlink-materialization, stale-history, package-set, obligation-status, or deterministic-order finding remains for the final reviewed source.

## Verification performed

- Two successful exact base/candidate reconciliations before the final hardening and one after it; all reported the same 3,390/3,432/218/3,640/406/620/66/132/153 results.
- Two independent final artifact generations were byte-identical. Comparison to the earlier pre-rebuild artifact differed only in the recorded build-specific exporter binary hash.
- Independent raw Git SHA-256 verification for every protected and candidate inventory source: zero mismatches.
- Independent deepest-formal-package/shared-input comparison for every emitted affected-package set: zero mismatches.
- Independent raw Git digest verification of all 142 retained review/actor records on both commits: zero mismatches; path sets identical.
- Disposable export-substitution and Git-symlink probes against `materialize`: raw bytes preserved and symlink rejected.
- `reconcile_tests.py`: 5 tests passed, covering raw export-substitution resistance, direct-tag rejection, existing output and output-symlink preservation, no publication after inventory failure, Git symlink rejection, and excessive-file rejection.
- Direct rejection probes for annotated-tag input, equal base/candidate commits, and an existing output path all returned code 2 with the intended diagnostic.
- Python source compiled without writing bytecode. No Cargo build, full test suite, proof gate, or hosted check was run in this review lane; the parent owns the single build lane.

## Remaining authorization work

This review validates only the pending reconciliation mechanism and its retained result. GAP-02 remains open. A real next PCR still needs a final frozen candidate, authentic owner and fresh independent reviewer provenance, all 132 exact passing gate records and outputs, complete findings/artifact inventory, a source-bound detached verdict, and the repository's separate authorization-then-application sequence against the actual protected base. All 153 obligations remain `in-progress` unless their own evidence and review contract is later satisfied.
