# docs/formal-coverage-evidence/release-candidate-construction — navigation

## Responsibility

Historical evidence for release candidate construction; recorded inputs, outputs and source identities belong to the earlier investigation.

## Files in this folder

| File | Role |
| --- | --- |
| [api-parent.txt](api-parent.txt) | Retained evidence/output for api parent. |
| [audit.md](audit.md) | Document: Release candidate construction correspondence. |
| [changed-source-sha256.txt](changed-source-sha256.txt) | Source identity digest inventory for the recorded run. |
| [clippy-parent.txt](clippy-parent.txt) | Retained evidence/output for clippy parent. |
| [fmt-parent.txt](fmt-parent.txt) | Retained evidence/output for fmt parent. |
| [full-package-sha256.txt](full-package-sha256.txt) | Retained evidence/output for full package sha256. |
| [independent-review.md](independent-review.md) | Document: Independent review of release candidate construction. |
| [integration-driver.py.txt](integration-driver.py.txt) | Retained evidence/output for integration driver.py. |
| [layout-parent.txt](layout-parent.txt) | Retained evidence/output for layout parent. |
| [preimage-sha256.txt](preimage-sha256.txt) | Retained evidence/output for preimage sha256. |
| [qualification-parent.json](qualification-parent.json) | Structured qualification parent data with fields `cwd`, `prior_files_checked`, `final_files_checked`, `changed_files`, `environment`. |
| [retention-driver.py.txt](retention-driver.py.txt) | Retained evidence/output for retention driver.py. |
| [review.json](review.json) | Structured review data with fields `schema`, `schema_version`, `status`, `recorded_at`, `base_commit`. |
| [source-diff.patch](source-diff.patch) | Retained evidence/output for source diff. |
| [tests-all-parent.txt](tests-all-parent.txt) | Retained evidence/output for tests all parent. |
| [tests-default-parent.txt](tests-default-parent.txt) | Retained evidence/output for tests default parent. |
| [verus-parent.txt](verus-parent.txt) | Retained evidence/output for verus parent. |
| [worker-baseline-to-final.patch](worker-baseline-to-final.patch) | Recorded comparison baseline. |
| [worker-baseline377-verify.txt](worker-baseline377-verify.txt) | Recorded comparison baseline. |
| [worker-clippy-03.txt](worker-clippy-03.txt) | Retained evidence/output for worker clippy 03. |
| [worker-clone-derived-probe-source.sha256](worker-clone-derived-probe-source.sha256) | Retained evidence/output for worker clone derived probe source. |
| [worker-clone-derived-probe-verus-02.txt](worker-clone-derived-probe-verus-02.txt) | Retained evidence/output for worker clone derived probe verus 02. |
| [worker-clone-derived-probe.rs.txt](worker-clone-derived-probe.rs.txt) | Retained evidence/output for worker clone derived probe.rs. |
| [worker-commands-20260912T001.txt](worker-commands-20260912T001.txt) | Retained evidence/output for worker commands 20260912T001. |
| [worker-contracts-report-20260912T001.md](worker-contracts-report-20260912T001.md) | Document: Release candidate constructor correspondence checkpoint. |
| [worker-evidence-sha256.txt](worker-evidence-sha256.txt) | Retained evidence/output for worker evidence sha256. |
| [worker-final-changed-source.sha256](worker-final-changed-source.sha256) | Retained evidence/output for worker final changed source. |
| [worker-final-full-package-manifest.sha256](worker-final-full-package-manifest.sha256) | Artifact/candidate/file inventory and binding metadata. |
| [worker-final-full-package-verify.txt](worker-final-full-package-verify.txt) | Retained evidence/output for worker final full package verify. |
| [worker-final-full-package.sha256](worker-final-full-package.sha256) | Retained evidence/output for worker final full package. |
| [worker-fmt-check-final-01.txt](worker-fmt-check-final-01.txt) | Retained evidence/output for worker fmt check final 01. |
| [worker-focused-layout-01.txt](worker-focused-layout-01.txt) | Retained evidence/output for worker focused layout 01. |
| [worker-negative-preimage.sha256](worker-negative-preimage.sha256) | Retained evidence/output for worker negative preimage. |
| [worker-negative-restored.sha256](worker-negative-restored.sha256) | Retained evidence/output for worker negative restored. |
| [worker-negative-verus-01.txt](worker-negative-verus-01.txt) | Retained evidence/output for worker negative verus 01. |
| [worker-ordinary-api-01.txt](worker-ordinary-api-01.txt) | Retained evidence/output for worker ordinary api 01. |
| [worker-post-clone-probe-freeze-diff.txt](worker-post-clone-probe-freeze-diff.txt) | Retained evidence/output for worker post clone probe freeze diff. |
| [worker-source-layout-01.txt](worker-source-layout-01.txt) | Retained evidence/output for worker source layout 01. |
| [worker-tests-all-features-final-01.txt](worker-tests-all-features-final-01.txt) | Retained evidence/output for worker tests all features final 01. |
| [worker-tests-default-final-01.txt](worker-tests-default-final-01.txt) | Retained evidence/output for worker tests default final 01. |
| [worker-verus-final-01.txt](worker-verus-final-01.txt) | Retained evidence/output for worker verus final 01. |
| [worker-workspace-freeze-diff.txt](worker-workspace-freeze-diff.txt) | Retained evidence/output for worker workspace freeze diff. |

## Working in this area

Treat this directory as retained evidence tied to the source identity and environment recorded by its owning report. Do not infer current behavior from an old log alone; follow the parent waypoint to the active implementation or replay and preserve original evidence bytes when adding newer observations.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
