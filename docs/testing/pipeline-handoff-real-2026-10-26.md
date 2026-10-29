# Pipeline handoff and OpenCode storage — real testing

Source baseline: `6fd71b9a257fa8943b44dbacd758696e1ca50350` (`v0.1.12`).
Changed source: branch `fix/issue-159-pipeline-handoff`.
Local observations: `/home/doll/.cache/peritus-issue159/`.

## Reported failure and cause

Issue [#159](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues/159) shows two final
replies promising to have the pipeline run a log-identity comparison, including after the user
says “go ahead.” Both are followed by Idle. The conversation coordinator accepted prose without
requiring its execution claims to agree with a successful `run_pipeline` receipt. This is a
host acceptance gap as well as a model instruction-following failure.

The developer tool executor now has an additive reply-validation hook. Existing executors retain
their completion preconditions through its default. The conversation executor rejects direct
first-person pipeline promises without a handoff receipt and returns a correction within the same
invocation. Local-memory and efficiency wrappers forward that hook. The correction cannot launch
work or grant authority; the actual handoff still checks current permissions and revision. The
Chat policy also explicitly resolves short approvals against the concrete preceding proposal.

## Observations

| Operation | Independently expected result | Observed |
| --- | --- | --- |
| Compile the actual host check and supply both screenshot promises | Require correction without executing anything | PASS |
| Fourteen host-check inputs covering wrapping, Unicode, direct execution verbs, quotes, code, conditional offers, explanations and blockers | Reject direct unfulfilled promises; retain informational/conditional replies | PASS |
| Actual Codex account: proposed comparison followed by “go ahead” | Execute the authorized task, write a filename-match report, preserve input files and `keep.txt` | Accepted; report read back and preservation checked |
| Actual Codex account: informational question quoting the misleading promise | Answer without entering the implementation pipeline | WaitingForUser with explanation; no report or design phase |
| Actual Codex account: Plan mode comparison workflow | Inspect/plan without implementation | WaitingForUser with plan; no report or design phase |
| Actual Zen/DeepSeek: same approved comparison under repaired runtime storage | Execute design, writing, checks, review and finish the report | Accepted; matching report read back and inputs preserved |
| Actual Zen/DeepSeek: disable session-runtime selection to reproduce the constrained shared-temp storage failure | Report a local storage cause and retain source files | `opencode.runtime.local_storage`, actionable quota/write-access guidance, no report or input mutation |

Fresh baseline Codex and DeepSeek attempts did call the handoff tool; the intermittent screenshot
failure was not reproduced on demand. The direct host-check replay establishes the new rejection
of its exact replies. The successful inference runs establish ordinary routing, execution and
read-only boundaries, not that a live model exercised the new promise-correction branch.

## Additional failure discovered and repaired

Baseline Zen/DeepSeek runs handed off and then failed during design with a generic provider error.
A transparent capture of unchanged official OpenCode stdout/stderr exposed:

```text
SQLiteError: disk I/O error
Failed to execute statement
```

The account's shared `/tmp` quota showed about 12,533 MiB used against a 12,602 MiB limit, while
`df` still showed free filesystem capacity. `/run/user/1000` had about 3.1 GiB available and was a
mode-0700 user runtime directory. The old adapter placed its private database/cache under the
quota-constrained shared temporary filesystem.

OpenCode now prefers a safe, absolute, non-symlink XDG session runtime directory, following the
official-account adapter's existing placement convention. The ambient-project rejection, private
credential files, disposable configuration, disabled native tools and automatic cleanup remain.
The successful real campaign's captured working directories were under
`/run/user/1000/peritus-opencode-*/config/opencode`; the shared `/tmp` quota was not cleaned up to
obtain that pass. When no suitable runtime directory is available, ordinary temporary-directory
fallback remains supported. Recognized SQLite storage errors on native stderr now produce a fixed,
redaction-safe local-storage diagnostic and specific recovery guidance rather than credential advice.

After obtaining the failure and recovery observations, `cargo clean` removed the two disposable
issue #155 consumer build directories created during earlier testing. Shared-temp quota usage
fell from about 12,533 MiB to 11,467 MiB. Their source manifests and saved reproductions were
preserved; the removed build output is regenerable.

## Build and replay

- `cargo clippy --locked --offline -p peritus-agent -p peritus-product-runner -p peritus-provider-opencode --all-targets --all-features -- -D warnings` passed with two build jobs.
- The public consumer was rebuilt against the changed libraries and invoked against absent,
  disposable output directories. Official Codex and official OpenCode 1.19.0 performed actual
  inference; no substitute providers or application services were used.
- The checked-in build script was then executed with the workspace lockfile and the retained
  consumer source. Its actual Zen/DeepSeek run completed every pipeline phase and reached
  Accepted; the final filename-match report and unchanged inputs were independently read back.
  The consumer also rejected a nonempty output directory before any provider work.
- Use `reproducers/open-issues/build-pipeline-handoff.py`, `pipeline-handoff.rs`,
  `pipeline-authorized.txt`, `pipeline-readonly.txt`, and `pipeline-promise.rs` as described in
  [their README](reproducers/open-issues/README.md).
- No unit, mock, formal-verification or repository qualification suites were run.

Coverage is bounded: this campaign exercised Linux, the named account/provider routes, a small
filename-comparison application task, and explicit read-only/Plan boundaries. The reply check is
an English promise guard, not a general natural-language authorization classifier. Windows/macOS
runtime placement and every possible provider phrasing were not exercised.
