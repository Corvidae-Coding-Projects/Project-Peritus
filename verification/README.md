# Verification manifest schemas

This directory contains the five authoritative, machine-readable inventories for Project Peritus's
review actors, trusted constructs, verification exclusions, proof obligations, and formal-source
change review. A1 establishes schema version 1. Trust, exclusion, and obligation entries are
explicitly empty; the actor registry resolves the A1 owner and independent reviewer, and the
proof-impact inventory records the reviewed A1 formal-semantics input identities. Empty never means
that source is trusted, verification coverage is complete, or an architectural invariant is
discharged.

All paths and source locations are repository-relative and use `/` separators. All dates are TOML
strings in the exact `YYYY-MM-DD` form. IDs, symbols, actor principals, issues, evidence, risks, and
contracts must name real, reviewable artifacts; sentinel values and prose placeholders are invalid.

## Common document envelope

The actor, trust, exclusion, and obligation documents have exactly four root fields:

| Field | Type | Contract |
|---|---|---|
| `schema` | string | Exact document identifier named below. |
| `schema_version` | integer | Exactly `1`; a new shape or meaning requires a new version. |
| `baseline` | string | Non-empty implementation slice or release baseline; `A1` for these files. |
| `entries` | array of tables | Zero or more records of the document-specific shape. |

Parsers must reject unknown fields at the document, entry, and nested-evidence levels. They must
also reject duplicate entry IDs, malformed dates or paths, blank strings, non-positive source
lines, and schema identifiers or versions they do not implement. A schema change updates the
identifier/version contract, compatibility fixtures, parser, and migration policy in one reviewed
change; readers must never guess at a newer schema.

An entry ID is stable after review. Renaming or moving its symbol updates its source coordinates
without recycling the ID. `owning_crate` must equal the Cargo package that owns `source_file`, and
`source_file` must be a regular, repository-owned file under that package with no symbolic-link
component. `symbol` is the fully qualified Rust or Verus item path. `source_line` is the positive
line on which the governed construct or item begins.

Issue fields contain the repository's canonical issue identifier or URL and must resolve to an open
issue at review/release time. Every `owner` and `reviewer` is a stable `ACTOR-NNNN` reference into
`actors.toml`. The checker requires the referenced role and distinct canonical provider subjects;
prose role descriptions are not identities. Evidence commands are exact, non-focused commands that
exercise the named evidence symbol from a clean locked dependency graph.

## Actor registry

`actors.toml` has schema identifier `peritus.verification.actors`. Each `[[entries]]` table has
exactly these fields:

| Field | Type | Contract |
|---|---|---|
| `id` | string | Unique stable nonzero `ACTOR-NNNN` identity used by other manifests. |
| `kind` | string enum | Exact provider class: `crosslink-agent` or `codex-subagent`. |
| `principal` | string | Immutable SSH-key fingerprint or repository/session/task identity. |
| `display_name` | string | Reviewable display label; never used as authority. |
| `roles` | array of string enums | Non-empty sorted unique set of `owner` and/or `reviewer`. |
| `provenance` | inline table | Exact `record_path` plus lowercase raw-byte `record_sha256`. |

Every provenance reference must name `verification/actor-provenance.json` and match its exact raw
bytes. That retained JSON record has schema `peritus.verification.actor-provenance`, version `1`,
baseline `A1`, and an `entries` array. Each entry has exactly `actor_id`, `kind`, `principal`,
`repository`, `issue`, `issue_created_at`, `session`, `task`, `mode`, optional `model` and
`reasoning_effort`, optional `public_key` and `allowed_signer`, and `record_locators`. Unknown fields
are rejected. The repository must be
`Corvidae-Coding-Projects/Project-Peritus`; issue and session IDs are positive; the issue timestamp
is its full UTC creation identity; and the task must be exactly `/root` or rooted beneath `/root/`.
Mode is `implementation` for an owner or `read-only-review` for a reviewer. A1 Codex reviewers must
record model `gpt-5.6-sol` with `xhigh` effort.

A Crosslink record embeds the exact OpenSSH ED25519 public-key and allowed-signer lines; the checker
decodes the key blob, recomputes its OpenSSH `SHA256:` fingerprint, requires the lines to name the
same key, and exposes them through sorted `embedded:` locators. This keeps the retained evidence in
an ordinary checkout rather than depending on Crosslink's custom refs. A Codex principal is the
canonical `<repository>/session/<positive-id>/task<absolute-task-path>` run identity, has no key
fields, and uses that same identity as its sole `codex-collaboration:` locator.
The registry rejects duplicate IDs, duplicate canonical provider subjects even when their
provenance differs, unregistered or extra provenance records, malformed principals, paths, hashes,
locators, issue/session identities, execution metadata, unknown fields, role mismatches, and
owner/reviewer aliases. Stable registry IDs allow a provider principal to be resolved without
treating a display label as a person.

Both `actors.toml` and `actor-provenance.json` are shared T+V proof-impact inputs, so remapping an ID
or changing identity, role, evidence, or provenance requires exact-hash independent review. The
content address makes the retained record tamper-evident but is not a self-authenticating Codex
signature; the independent reviewer's final read-only verdict remains detached Gate A evidence.

## Nested evidence records

Trust and exclusion entries contain `evidence`, an array of tables with exactly these fields:

| Field | Type | Contract |
|---|---|---|
| `kind` | string enum | One of `refinement-test`, `conformance-test`, `fault-injection-test`, or `model-check`. |
| `source_file` | string | Repository-relative regular source or test file. |
| `symbol` | string | Fully qualified test, model, or proof symbol in that file. |
| `command` | string | Exact locked command that executes or verifies the evidence. |

Every evidence location must exist and every command must select the owning package without a
focused verification mode, ignored failure, or skip. A Cargo test locator must name an
unconditionally compiled `#[test]`; a Cargo-Verus locator must name an exact proof/spec item. An
evidence record is a locator, not a claim that the command passed; CI and release evidence record
the actual result separately.

### Trusted-construct manifest

`trust.toml` has schema identifier `peritus.verification.trust`. Each `[[entries]]` table has
exactly these fields:

| Field | Type | Contract |
|---|---|---|
| `id` | string | Unique stable ID in `TRUST-NNNN` form. |
| `symbol` | string | Exact symbol containing the trusted occurrence. |
| `owning_crate` | string | Must be `peritus-tcb`. |
| `source_file` | string | Exact file under `crates/foundation/peritus-tcb/`. |
| `source_line` | integer | Exact positive line of the occurrence. |
| `construct_kind` | string enum | Exact lexical construct kind listed below. |
| `upstream` | string | Project, library, ABI, or platform whose behavior is modeled. |
| `upstream_version` | string | Exact immutable revision, release, ABI, or platform-version range reviewed. |
| `assumed_contract` | string | Minimal behavior supplied to Verus, without a desired domain outcome. |
| `threat_if_false` | string | Concrete guarantee or safety property invalidated if the contract is false. |
| `evidence` | array of evidence tables | Non-empty; includes a refinement or conformance test and applicable fault injection. |
| `live_issue` | string | Open issue for elimination, upstream proof, or continuing audit. |
| `owner` | string | Registered actor with the `owner` role accountable for the boundary. |
| `reviewer` | string | Distinct registered actor with the `reviewer` role. |
| `review_date` | date string | Date the entry and evidence were last reviewed. |
| `expiry_date` | date string | Mandatory future re-review deadline, later than `review_date`. |

`construct_kind` is one of the following exact values, matching the A0 source scanner:

- `assume`
- `admit`
- `axiom`
- `assume_specification`
- `external`
- `external_body`
- `external_fn_specification`
- `external_type_specification`
- `external_trait_specification`
- `external_trait_extension`
- `external_trait_private_bound`
- `external_derive`
- `external_trait_blanket`
- `verus::trusted`
- `verifier::assume_termination`
- `verifier::exec_allows_no_decreases_clause`
- `exec_spec_unverified`
- `inline_air_stmt`
- `allow-inline-air`
- `builtin::assume_`
- `Ghost::assume_new`
- `Ghost::assume_new_fallback`
- `Tracked::assume_new`
- `Tracked::assume_new_fallback`
- `*::assume_new` (fail-closed fallback for an unresolved/aliased receiver)
- `*::assume_new_fallback` (fail-closed fallback for an unresolved/aliased receiver)

Explicit imports, reexports, glob imports from `vstd::pervasive`, public prelude glob reexports,
and aliases of trusted operations or `Ghost`/`Tracked` constructors are prohibited even in
`peritus-tcb`. Canonical call spellings are required so calls remain independently countable.
Trusted constructs nested in inline modules, impls, traits, or type declarations are also
prohibited; A1 permits only file-level items so exact symbol ownership cannot be obscured by lexical
scope. These rules include the proof-forging public constructors exposed by the pinned builtin
revision while leaving `Ghost::new`, `Tracked::new`, `ghost_exec`, and `tracked_exec` ordinary:
those accept an existing spec/proof value rather than manufacturing one.

Semantic reconciliation is one-to-one: every scanned trusted occurrence must match one entry by
crate, file, line, symbol, and kind, and every entry must match one current occurrence. The checker
must reject a construct outside `peritus-tcb`, an expired entry, an unpinned upstream version, or
evidence that does not include at least one `refinement-test` or `conformance-test`.

### Verification-exclusion manifest

`exclusions.toml` has schema identifier `peritus.verification.exclusions`. Each `[[entries]]` table
has exactly these fields:

| Field | Type | Contract |
|---|---|---|
| `id` | string | Unique stable ID in `EXCL-NNNN` form. |
| `symbol` | string | Exact H/T function or item excluded from proof. |
| `owning_crate` | string | Registered Cargo package that owns the item. |
| `source_file` | string | Exact source file owned by that package. |
| `source_line` | integer | Exact positive line at which the item begins. |
| `verification_class` | string enum | Exactly `H` or `T`. |
| `unsupported_feature` | string | Specific pinned-Verus limitation preventing verification. |
| `risk` | string | Concrete guarantee or invariant left outside formal proof. |
| `evidence` | array of evidence tables | Non-empty compensating evidence; never described as a proof discharge. |
| `live_issue` | string | Open repository issue tracking removal or reduction of the exclusion. |
| `owner` | string | Registered actor with the `owner` role accountable for the excluded item. |
| `reviewer` | string | Distinct registered actor with the `reviewer` role. |
| `review_date` | date string | Date the limitation, risk, and evidence were reviewed. |
| `upstream_tracking` | string | Exact upstream issue, documentation section, or immutable source reference. |
| `revisit_plan` | string | Concrete condition and work required to bring the symbol into verification. |
| `revisit_by` | date string | Mandatory future review deadline, later than `review_date`. |

Exclusions are not trust allowlists. They describe deterministic H/T code that a supported Verus
feature cannot presently express; they do not authorize a trusted construct. The checker must
reject V/C entries, expired revisit dates, broad module/crate exclusions, missing compensating
evidence, or an owning package/class mismatch with the architecture registry.

### Proof-obligation manifest

`obligations.toml` has schema identifier `peritus.verification.obligations`. Each `[[entries]]`
table has these common fields:

| Field | Type | Contract |
|---|---|---|
| `id` | string | Unique stable obligation or architectural invariant ID. |
| `kind` | string enum | One of `invariant`, `contract`, `refinement`, `termination`, or `liveness`. |
| `statement` | string | Precise property to prove; it cannot be replaced by a test description. |
| `owning_crate` | string | Registered package responsible for the proof. |
| `source_file` | string | Exact file intended to own or currently owning the proof. |
| `symbol` | string | Fully qualified decision/proof symbol governed by the obligation. |
| `status` | string enum | One of `open`, `in-progress`, `discharged`, or `excluded`. |
| `dependencies` | array of strings | Other obligation IDs required first; empty when independent. |
| `live_issue` | string | Open issue carrying the work or maintaining the proof. |
| `owner` | string | Registered actor with the `owner` role accountable for the obligation. |
| `evidence` | array of proof-evidence tables | Empty until evidence exists; status rules below apply. |

Proof-evidence tables have exactly `kind`, `source_file`, `symbol`, and `command`. Their `kind` is
one of `verus-proof`, `model-check`, `refinement-test`, or `property-test`; the remaining fields
follow the nested-evidence meanings above.

Three fields are conditional and otherwise must be absent:

| Field | Type | Contract |
|---|---|---|
| `reviewer` | string | Required only for `discharged`; distinct registered reviewer of the proof and statement. |
| `review_date` | date string | Required only for `discharged`; date the recorded evidence was reviewed. |
| `exclusion_id` | string | Required only for `excluded`; ID of the matching live exclusion record. |

An `open` or `in-progress` entry has no reviewer, review date, or exclusion ID and may have an empty
evidence array. A `discharged` entry requires non-empty evidence including `verus-proof` or
`model-check`, plus its reviewer and review date, and has no exclusion ID. An `excluded` entry has
one matching `EXCL-NNNN` reference, no discharge reviewer/date, and cannot be counted as proved.

Dependencies must reference existing obligation IDs, contain no duplicates, and form an acyclic
graph. Every `INV-001` through `INV-022` remains not discharged until its owning slice adds a real
entry and reviewed proof evidence. Their absence from the A1 list is deliberately not a coverage or
success claim.

### Proof-impact and change-review manifest

`proof-impact.toml` has schema identifier `peritus.verification.proof-impact`. It has exactly these
root fields: `schema` (string), `schema_version` (integer `1`), `baseline` (`A1`),
`hash_algorithm` (`sha256-raw-bytes-v1`), `sources` (array of semantics-input tables), and `changes`
(array of review tables). Unknown fields at every level are rejected.

Each `sources` table has exactly `source_file` (exact repository path), `sha256` (lowercase 64-hex
digest of its raw bytes), `affected_packages` (non-empty canonical array of affected-package
tables), and `change_id` (the approved review record containing the exact current identity). Each
affected-package table has exactly `package` (exact Cargo package name) and `verification_class`
(`V`, `H`, or `T`). The array is strictly sorted and contains no duplicates. For current inputs the
packages and classes must exactly match `architecture.toml`: package-local inputs name their exact
package, while shared inputs name every current V/H/T package they affect.

The inventory is exhaustive in both directions. It covers every Cargo-reachable source and package
`Cargo.toml` for V/H/T packages, plus the shared inputs `Cargo.toml`, `Cargo.lock`,
`.cargo/config.toml`, `rust-toolchain.toml`, `toolchains.toml`, `architecture.toml`, and
`verification/actor-provenance.json` and
`verification/{actors,trust,exclusions,obligations}.toml`. Tests are compilation inputs; manifests,
dependency resolution, toolchain selection, architecture registration, and verification policy
alter executable or verification semantics. `proof-impact.toml` is deliberately not self-hashed;
its protected-base append-only comparison provides its history boundary without a recursive hash.

Each `[[changes]]` table has exactly:

| Field | Type | Contract |
|---|---|---|
| `id` | string | Unique immutable nonzero `PCR-NNNN` identity. |
| `status` | string enum | `approved` or `revoked`; current source may reference only `approved`. |
| `change_kinds` | array of string enums | Exactly `executable`, `specification`, `precondition`, `postcondition`, and `proof` in A1. |
| `source_changes` | array of tables | Exact identity transition(s), each with `source_file` and optional `previous`/`current` snapshots. |
| `rationale` | string | Why the exact change is needed. |
| `impact` | string | Review of semantic, contract, proof, and invariant consequences without claiming unproved invariants. |
| `evidence` | array of tables | Exactly one ordinary test and one class-correct Cargo-Verus command per affected package. |
| `owner` | string | Registered accountable actor with the `owner` role. |
| `reviewer` | string | Distinct registered actor with the `reviewer` role. |
| `review_date` | date string | Completed review date; future dates are rejected. |

Each `previous` or `current` snapshot has exactly `sha256` and `affected_packages`. Snapshot package
names and formal classes remain immutable historical identities; they need not equal the later
current architecture after a reviewed reclassification or retirement. A transition omits
`previous` only when the input was absent and omits `current` only when removal was reviewed.
Identical snapshots and broken identity chains fail. Tracking the package/class set in the identity
makes scope changes review-visible even when the input bytes do not change. Evidence covers the
union of the previous and current affected sets and is checked against each recorded class; tables
have exactly `kind` (`ordinary-test` or `verus-verify`), `owning_crate`, and `command`. V/H
verification evidence includes `--no-cheating`; T verification evidence omits it so
manifest-accounted trusted bodies remain possible. Ordinary tests alone never satisfy formal
change evidence.

Raw-byte hashing deliberately treats formatting and comments as review-visible. A1 does not trust
a partial Rust/Verus parser, so every byte or affected-scope change conservatively requires review
under all five impact kinds. The immutable fingerprint authorization does not expire; a later
decision is appended under a new ID. Protected review records remain an exact prefix, so they
cannot be deleted, rewritten, reordered, or preceded by newly inserted history.

The A1 records are the one-time genesis because the protected base has no earlier inventory. After
genesis, a transition is accepted only if its exact approved record already exists unchanged on the
protected Git base. This two-step rule prevents a source edit from approving its own review record.
The genesis exception is rejected when any parent history of `HEAD` has already contained the
manifest, preventing an older base selection from reopening bootstrap after A1.
On GitHub Actions, `PERITUS_PROOF_IMPACT_BASE` is mandatory, must be a nonzero full 40-hex commit,
must resolve locally, must be an ancestor of and differ from `HEAD`; CI fetches full history and
selects the pull-request base, push-before SHA, or required dispatch input. Local checks use `HEAD`
when the variable is unset. This inventory makes no `INV-001` through `INV-022` discharge claim.

## Integration requirements

The semantic manifest check must:

1. Parse each file with unknown-field denial and exact schema/version matching.
2. Validate every common and document-specific structural rule above, including actor resolution,
   roles, provenance, uniqueness, and independence.
3. Reconcile trust occurrences one-to-one with `trust.toml` and reject them everywhere else.
4. Reconcile exclusion owners/classes with Cargo metadata and `architecture.toml`.
5. Validate cross-references, uniqueness, obligation status semantics, and the dependency DAG.
6. Treat missing required inventories, evidence, live issues, expired reviews, and untracked
   deterministic decisions as failures, never warnings or implicit discharge.
7. Hash every formal semantics input, validate its exact affected package/classes, and compare input
   transitions and immutable review history against the protected Git base under the rules above.

Local and repository CI checks validate canonical issue syntax and exact executable command forms;
they do not make an unauthenticated network-liveness claim. Independent protected-branch/release
review confirms issue liveness. The canonical workspace Gate A commands execute the test and
class-correct verification superset represented by proof-impact evidence against the reviewed
revision.
