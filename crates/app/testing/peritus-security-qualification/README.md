# peritus-security-qualification

`peritus-security-qualification` owns the C-class H0 campaign boundary. It runs a closed security
catalog against one exact integrated candidate, provisions a fresh native subject for every probe,
enforces resource and cleanup accounting, packages canonical evidence, and delegates the final
deterministic decision to `peritus-security-policy`.

The campaign covers malicious repositories and paths, symlink/worktree/device/case aliases, shell,
prompt, terminal, and secret attacks; tier-one sandboxes and network policy; plugin and MCP
authority; evolution isolation and promotion/rollback; supply-chain attestations; unsafe and TCB
inventories; and finding closure.

## Trust boundary

The crate does not contain a fake native runner and does not create an external-review record.
Host adapters implement `FreshSubjectFactory`/`QualificationSubject`; an independent party supplies
`IndependentSecurityReview`. Adapter errors, unsupported facilities, cancellation, missing cleanup,
reused subjects, empty evidence, incomplete review scope, and unresolved blockers all remain
non-success.

`NativeProbeFactory` supplies the standard process boundary for those host adapters. It creates a
new private root for every case, sends the exact candidate, probe, and limits to a reviewed native
executor through bounded JSON, owns and terminates that process, validates candidate-bound
structured evidence and the exact bytes of every named raw artifact, and removes the subject root
before reporting cleanup. A separate retained-artifact root named by the fresh subject ID survives
cleanup. The executor still has to implement the real platform probe; returning `unsupported` or
failing to produce a valid bound response remains a failed case. This boundary does not create or
stand in for an independent security review.

For every case, the factory invokes the reviewed executor as:

```text
<executor> \
  --request <subject-root>/request.json \
  --response <subject-root>/response.json \
  --subject-root <subject-root> \
  --artifact-root <retained-root>/<fresh-id> \
  --subject-id <fresh-id> \
  --request-sha256 <sha256>
```

The request and response formats are compiled into the crate and published under
`security/schemas/`. Digest evidence must name a portable relative path beneath the assigned
artifact root. The adapter rejects links, escapes, duplicate paths, byte-count mismatches, digest
mismatches, and disagreement between resource accounting and the number of retained files.
Standard output and standard error are drained but never accepted as evidence. The adapter starts a
separate Unix process group or Windows kill-on-close Job Object, so timeout, cancellation, and drop
terminate descendants before the runner records cleanup.

`ReadinessVerdict::Ready` is an H0 qualification result only. It is not H4 release authority.

Production execution is partitioned into three candidate-bound native shards. Linux owns the 39
portable tier-one probes plus the Linux backend probe; macOS and Windows each own only their native
backend probe. Aggregation accepts exactly one shard from every platform, requires identical
candidate and resource-limit bindings, rejects duplicate subject or probe evidence, and restores
the original 42-case catalog order before policy evaluation. No host can stand in for another
operating system.

Each shard has a deterministic JSON projection described by
`security/schemas/native-shard-v1.schema.json`. The parser reconstructs the exact candidate,
limits, native receipts, evidence entries, and cleanup observations. It admits only a passing,
internally consistent canonical shard; a failed report can be retained for diagnosis but cannot be
aggregated into H0 readiness.

## Evidence

`EvidenceManifest` produces deterministic JSON in stable probe order and hashes those exact bytes
with SHA-256. Evidence values are structured facts, counts, digests, or bounded canonical codes;
arbitrary stdout, terminal text, model output, and secrets are retained only by digest outside the
manifest.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-security-qualification
```
