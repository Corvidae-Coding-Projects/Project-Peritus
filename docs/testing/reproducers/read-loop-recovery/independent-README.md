# Independent read/reuse oracle and archive-policy regression

`independent_archive_policy.py` operates the real native public-API consumer in this directory. It creates disposable files/state, sends actual NDJSON actions, closes/reopens the native process, and compares results with fixture-derived expectations. It does not run provider inference, unit tests, mocks, or a substitute implementation.

```sh
python3 docs/testing/reproducers/read-loop-recovery/independent_archive_policy.py \
  --consumer /absolute/path/to/consumer \
  --output /new/disposable/output
```

The output directory must not already exist. Every run records the consumer SHA-256, exact actions/results, fixtures, oracle handles, stderr, and a compact result. Exit 0 means all cases pass; a mismatch or blocked setup exits 1. Consumer compilation/build provenance belongs to `build.py` and the caller.

The preserved failure is an archive path around a current private exclusion. Read a local file or an exact user-named external reference, retrieve its archive, retain an explicitly source-supported observation, checkpoint, and reopen the same persisted scope with that source in `private_paths`. The required result is that current source-backed bytes remain inaccessible through:

- The original ToolOutput handle and its actual encoded ToolMessage wrapper.
- An earlier `context_read` output and a second retrieval linked to that output.
- Literal search results naming those known private source identities.
- The linked working entry in assembled context and `context_read` state pages.

The driver retrieves and validates actual wrapper/receipt source bytes before using their handles. Its expected source content comes from the input fixture. It decodes UTF-8/hex source pages independently, so an encoded wrapper cannot evade the marker check. Search may still return unrelated free-form prose, including a past search query or assistant entry proposal; this is a source-provenance policy check, not general prose taint erasure.

`independent-original-archive-policy.json` retains the observed mismatch from the original consumer; `independent-final-archive-policy.json` records both local and external cases passing on final consumer SHA-256 `75fc6a3a3623e1e39235e9f288a0abf23da4cd9db37d2f8298e2cb11f1756e2e`. The original consumer's separate external-body visibility defect produced a stale receipt; the replayer can use its actual archive handle and still validates the archived bytes against the fixture. It does not accept the stale verdict as correct.

The broader independent final campaign executed 302 public-API commands across 38 native process launches, all passing with empty stderr. It covered exact UTF-8/line ranges, 18-page reconstruction of a 2,951-byte long line, visible-range reuse for relative/external references, same-size edits with restored mtime, missing/recreated/atomically replaced files, host Read and actual chmod(000) revocation, cold writer continuation, reviewer/source-scope isolation, and repeated `./` aliases. Explicit `././` private, traversal, `.git`, and symlink paths were rejected. Twelve varied local/external Unicode/space-path range sequences used seed `202610204719`; their exact expected content was generated before requests from independent fixture lines. These successful bulk transcripts remain outside the repository.


The historical branch has a separate portable replay. Supply the stopped completed-run spec whose trace directory retains the writer context, along with its existing workspace after the target was removed:

```sh
python3 docs/testing/reproducers/read-loop-recovery/independent_historical_policy.py \
  --consumer /absolute/path/to/consumer \
  --source-spec /completed-run/spec.json \
  --target temporary-operation.json \
  --output /new/disposable/historical-output
```

It copies the trace/private directory and workspace before opening the consumer, uses a fresh command-state directory, and forces `live=false` and `restore_existing=true`. Its control preserves the exact historical read before Read revocation hides it. Its private copy excludes that body and its original archive handle. It requires one recorded workspace_write for the target and derives the expected bytes from that request. The source run and its deleted target remain untouched. This portable replay passed five API commands across two native launches on the final artifact; the earlier equivalent exploratory check is retained outside the repository.

A corrected early oracle assumption is preserved here for interpretation: baseline in-place private paths were workspace-scoped, so adding an outside-root sibling did not reject either fresh raw or cached reads. That was not a cached-read-only regression. The outside-root exclusion was subsequently strengthened and verified after reopening with current `private_paths`. The distinct confirmed archive defect then showed that even after file reads and ordinary views honored the exclusion, explicit retrieval could still expose archived source bytes; the saved replayer verifies the repaired policy and linked retrieval paths.

A separate final replay reopened two disposable copies of an actual completed provider run, adding five API commands and two process launches (307 commands and 40 launches total). Its control preserved the deleted temporary file's exact successful read as explicitly historical evidence; expected bytes came from the recorded workspace_write input. Revoking Read hid that historical body. The protected copy hid it from both assembly and exact archive retrieval. The temporary file stayed deleted in both copies, and no model inference or original-state mutation occurred. The compact final JSON includes these outcomes.

The independent source review read `activity.rs`, `assembly/continuation.rs`, `tools.rs`, `assembly/exchanges.rs`, `assembly/transport.rs`, and `read_reuse/view.rs` in full and followed relevant source-policy and page-slicing callers. No new concrete policy or error defect was found in the historical replacement handling. It did not review the entire package. These checks establish the native memory/workspace API behavior; they do not establish UI, full daemon, or model-inference behavior.
