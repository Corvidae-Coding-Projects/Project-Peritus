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

`ReadinessVerdict::Ready` is an H0 qualification result only. It is not H4 release authority.

## Evidence

`EvidenceManifest` produces deterministic JSON in stable probe order and hashes those exact bytes
with SHA-256. Evidence values are structured facts, counts, digests, or bounded canonical codes;
arbitrary stdout, terminal text, model output, and secrets are retained only by digest outside the
manifest.
