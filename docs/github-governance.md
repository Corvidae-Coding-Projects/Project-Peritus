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

Current external contracts to recheck during activation or migration:

- <https://docs.github.com/en/rest/repos/rules?apiVersion=2026-03-10>
- <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/creating-rulesets-for-a-repository>
- <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
- <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches>
