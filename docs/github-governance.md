# GitHub Gate A governance

Project Peritus currently uses the strongest practical GitHub Team enforcement for Gate A. An
active repository ruleset targets `main`, permits no bypass actors, requires pull requests, blocks
deletion and non-fast-forward updates, and requires the strict, up-to-date `Gate A` status emitted
by `.github/workflows/formal-governance.yml`.

The canonical repository-ruleset payload is
`docs/formal-governance-ruleset.template.json`. `cargo xtask reproducibility-check` rejects a
missing, symbolic, or byte-drifted template. The checked workflow has a stable final job named
`Gate A`; that job runs even after dependency failure and succeeds only when candidate policy,
workflow lint, every Rust matrix entry, supply-chain policy, and every strict Verus/no-cheating
operation succeeded.

Gate A cancels superseded runs only for the same pull request. Main pushes and merge-queue
commits use separate concurrency groups and are not canceled by later commits. Every check and
failure-propagating aggregator remains required on the current candidate.

## Current authority boundary

GitHub Team supports repository rulesets and required status checks for private repositories. The
ruleset can bind the `Gate A` check to the GitHub Actions application, preventing another status
producer from satisfying the requirement. It cannot pin the defining workflow to a separately
reviewed immutable commit. A candidate with permission to change both the workflow and its policy
checker can therefore weaken future enforcement in the same pull request.

That residual risk is accepted as an explicit, budget-driven deferral. Local Gate A, independent
review, proof-impact review, and the complete CI workload remain mandatory. The Enterprise Cloud
upgrade is deferred, not silently treated as equivalent: when available, replace this status-check
rule with a required-workflow rule pinned to an independently reviewed authority revision.

GitHub also documents that people or integrations with repository write access can otherwise set
status results. For this reason, activation must discover the actual `Gate A` check run's GitHub
Actions application ID and add it as `integration_id`; do not activate an any-source check.

The ruleset requires a pull request but zero GitHub approvals because this is presently a
solo-maintainer repository and GitHub does not permit authors to approve their own pull requests.
Independent agent review remains required by the project process, but GitHub Team does not enforce
that detached review. Limit repository write access accordingly: a write collaborator could alter
the candidate-controlled workflow, satisfy the weakened check, and merge their own pull request.

## Trusted-base validation available in repository code

`.github/workflows/formal-authority.yml` defines a read-only `pull_request_target` validation for
pull requests targeting `main` or `develop`. Once that file is present on the default `main` branch,
GitHub obtains its workflow definition from the default branch rather than from the pull-request
head. GitHub defines `github.sha` and `github.ref` for this event as the last commit and ref on the
default branch, while `github.workflow_sha` and `github.workflow_ref` identify the exact workflow
file. The job binds those values to an immutable checker checkout and separately binds the exact PR
base and head to comparison-base and candidate checkouts. It requires the PR base to be an ancestor
of the candidate and uses no candidate action or script.

The candidate checkout explicitly opts into the checkout action's fork checkout because this job
treats the candidate tree as untrusted data: it persists no credentials and executes no candidate
action or program. Before invoking Cargo in the candidate checkout, the job rejects every authority,
base, or candidate
Git tree entry except a regular file and rejects the legacy `.cargo/config` and `rust-toolchain`
selectors. It requires the root Cargo configuration, attributes, toolchain selector, manifest, and
lockfile to have exact bytes and modes across both custody edges: checker revision to PR base, then PR
base to candidate. The same two comparisons cover `xtask`'s manifest, source tree, possible default
build script, and every current non-test repository input embedded in the checker binary. For a
`main` pull request, the checker and PR base SHAs must also be identical. For a `develop` pull request,
their commits may differ, but every protected authority input must remain identical.

`RUSTUP_TOOLCHAIN` and a runner-temporary `CARGO_HOME` are set by the trusted workflow. The job builds
`xtask` from the exact workflow/checker checkout into `RUNNER_TEMP`, primes only that revision's
locked dependency metadata, and then runs that exact binary against the candidate with offline Cargo
metadata and a PATH reconstructed from the resolved runner Cargo and Git directories plus fixed
system binary directories. `all` runs before `verify-trust`. The latter receives the exact PR base
SHA, not the checker SHA, as `PERITUS_PROOF_IMPACT_BASE`, preserving the candidate's actual
proof-impact comparison and authorization base.

`cargo metadata` reads candidate manifests and dependency metadata but does not compile candidate
build scripts or load candidate procedural macros. Root manifest, lockfile, or authority-checker
drift fails before metadata, and the offline boundary also prevents resolution outside the checker's
primed closure. There is no label, actor, approval count, candidate record, or workflow input that
bypasses either custody comparison.

Before exclusive App enforcement is enabled, a checker or dependency transition needs an independent
review bound to the exact new head and complete protected-input tree, followed by a separately
controlled exact-head bootstrap. That bootstrap must establish the reviewed checker inputs on
default `main` and then establish identical protected inputs on `develop`; ordinary develop
validation remains fail-closed until the two authority surfaces agree. After exclusive App
enforcement, the App producer must support an equivalent separately controlled exact-head bootstrap
decision. Neither route may silently retry candidate resolution online or treat candidate-provided
review files as independent authorization.

This workflow currently produces an ordinary GitHub Actions check. It does not make the required
status exclusive because candidate workflows can publish checks through the same GitHub Actions
application identity. No App reporter or App credential is present in this repository increment.
The code is therefore validation-ready and enforcement-incomplete.

The checker built from `github.workflow_sha` embeds the exact reviewed authority-workflow bytes. It
rejects any PR-base or candidate change to that workflow or to the repository-controlled checker
build inputs even when review or policy files change at the same time. A legitimate workflow or
checker update must use the explicit external bootstrap above. Candidate-provided records cannot
authorize it.

## Exclusive authority deployment prerequisite

Exclusive enforcement requires a dedicated GitHub App installed on the repository with only the
permission needed to publish its check. A separately controlled reporter must consume the exact
trusted validation result, bind the repository, base SHA, head SHA, trusted-checker revision, and
conclusion, and publish its own check on that head. Failure, cancellation, absence, identity
mismatch, or an unrecognized checker or dependency bootstrap decision must produce a non-success
result. The App private key must remain outside candidate-accessible jobs and workflows.

After the App has published a real check, update the main ruleset to require that exact context
with the App's `integration_id`. Retain zero required GitHub approvals: the maintainer still reviews
and self-merges through the pull-request path after the checks pass. Installing the App, deploying
the reporter, and changing the live ruleset are external administration steps and are not performed
by the checked workflow.

The pull request that first introduces `formal-authority.yml` cannot be validated by itself because
`pull_request_target` loads only the version already on the default branch. Bootstrap it through the
existing Gate A path after independent review of the exact commit, then exercise the trusted-base
job on a later pull request before configuring the dedicated App check as required. Do not claim
exclusive authority from a local pass or from the ordinary Actions result during that interval.

## Genesis sequence

A required check must exist before its source application can be selected. For the initial A1
landing only:

1. confirm A0 is already the parent of the reviewed A1 commit;
2. push that exact signed A1 commit directly to the currently unprotected `main`;
3. wait for the `Gate A` workflow on `main` to complete successfully;
4. discover and verify the check-run source application; and
5. immediately create and verify the active repository ruleset.

Do not make another unprotected `main` change between steps 2 and 5.

Set the exact pushed commit and confirm GitHub resolves it:

```text
export PERITUS_A1_SHA=<40-hex-a1-commit>
test "${#PERITUS_A1_SHA}" -eq 40
gh api \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  "repos/Corvidae-Coding-Projects/Project-Peritus/git/commits/${PERITUS_A1_SHA}" \
  --jq '.sha'
```

Resolve the official GitHub Actions application independently, then prove that exactly one
successful `Gate A` check on the A1 commit came from that application. Retain the check-run response
as evidence:

```text
export PERITUS_ACTIONS_APP_ID="$(gh api \
  -H 'Accept: application/vnd.github+json' \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  apps/github-actions \
  --jq 'if .slug == "github-actions" and .owner.login == "github"
    then .id else error("unexpected GitHub Actions app identity") end')"
test "${PERITUS_ACTIONS_APP_ID}" -gt 0
gh api \
  -H 'Accept: application/vnd.github+json' \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  "repos/Corvidae-Coding-Projects/Project-Peritus/commits/${PERITUS_A1_SHA}/check-runs" \
  > check-runs-evidence.json
jq -e --argjson app_id "${PERITUS_ACTIONS_APP_ID}" '
  [.check_runs[] | select(
    .name == "Gate A" and
    .conclusion == "success" and
    .app.id == $app_id and
    .app.slug == "github-actions" and
    .app.owner.login == "github"
  )] | length == 1
' check-runs-evidence.json
```

The final predicate must print exactly `true`.

Materialize the request in a temporary file. The checked-in template deliberately omits the
environment-specific application ID:

```text
export PERITUS_RULESET_PAYLOAD="$(mktemp)"
jq --argjson app_id "${PERITUS_ACTIONS_APP_ID}" '
  (.rules[] | select(.type == "required_status_checks") |
    .parameters.required_status_checks[0].integration_id) = $app_id
' docs/formal-governance-ruleset.template.json > "${PERITUS_RULESET_PAYLOAD}"
jq -e --argjson app_id "${PERITUS_ACTIONS_APP_ID}" '
  .enforcement == "active" and
  .bypass_actors == [] and
  any(.rules[];
    .type == "required_status_checks" and
    .parameters.strict_required_status_checks_policy == true and
    .parameters.do_not_enforce_on_create == false and
    .parameters.required_status_checks == [{
      "context": "Gate A",
      "integration_id": $app_id
    }])
' "${PERITUS_RULESET_PAYLOAD}"
```

The predicate must print exactly `true`. Create the repository ruleset; this requires repository
Administration write access but not organization `admin:org` scope or Enterprise Cloud:

```text
gh api --method POST \
  -H 'Accept: application/vnd.github+json' \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  repos/Corvidae-Coding-Projects/Project-Peritus/rulesets \
  --input "${PERITUS_RULESET_PAYLOAD}"
```

## Required-state verification

Record the returned ruleset ID, fetch the active response, and retain it as Gate A evidence:

```text
export PERITUS_RULESET_ID=<ruleset-id>
gh api \
  -H 'Accept: application/vnd.github+json' \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  "repos/Corvidae-Coding-Projects/Project-Peritus/rulesets/${PERITUS_RULESET_ID}" \
  > ruleset-evidence.json
jq -e --argjson app_id "${PERITUS_ACTIONS_APP_ID}" '
  .name == "Project Peritus Gate A" and
  .source_type == "Repository" and
  .source == "Corvidae-Coding-Projects/Project-Peritus" and
  .target == "branch" and
  .enforcement == "active" and
  .bypass_actors == [] and
  .conditions.ref_name.include == ["refs/heads/main"] and
  .conditions.ref_name.exclude == [] and
  any(.rules[]; .type == "deletion") and
  any(.rules[]; .type == "non_fast_forward") and
  any(.rules[];
    .type == "pull_request" and
    .parameters.allowed_merge_methods == ["merge"] and
    .parameters.required_review_thread_resolution == true) and
  any(.rules[];
    .type == "required_status_checks" and
    .parameters.strict_required_status_checks_policy == true and
    .parameters.do_not_enforce_on_create == false and
    .parameters.required_status_checks == [{
      "context": "Gate A",
      "integration_id": $app_id
    }])
' ruleset-evidence.json
```

The result must be exactly `true`. Also retain the successful genesis workflow URL and A1 commit
SHA. Then test the rule with a small pull request: the PR must show `Gate A` as required, a stale
head must require retesting, and a failed or absent `Gate A` result must prevent merging.

## Team-era changes

Every workflow, checker, action pin, tool pin, or ruleset-template change remains a governance
change:

1. branch from protected `main` and make one cohesive change;
2. run local Gate A and obtain fresh independent review of the exact tree;
3. push to the branch and require the complete remote `Gate A` result;
4. merge only through the protected pull-request path; and
5. retain the review, check run, merge commit, and ruleset evidence.

Because Team enforcement is candidate-controlled, reviewers must inspect any change to
`.github/workflows/`, `xtask`, `justfile`, toolchain pins, verification manifests, or this ruleset
template before merge. Never disable enforcement, add a bypass actor, accept an any-source status,
or push directly to protected `main`.

After `formal-authority.yml` is present on `main`, its trusted-base result provides an additional
read-only check of those changes. Until the dedicated App reporter and source-bound ruleset entry
are deployed, the existing Gate A check and maintainer self-merge remain the live enforcement path.

Current external contracts to recheck during activation or migration:

- <https://docs.github.com/en/rest/repos/rules?apiVersion=2026-03-10>
- <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/creating-rulesets-for-a-repository>
- <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
- <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches>
