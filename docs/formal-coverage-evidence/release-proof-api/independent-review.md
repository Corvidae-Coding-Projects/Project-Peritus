# Independent source review: release proof API and diagnostic text

Verdict: **PASS for the exact two-file increment**. This review does not approve the wider release goal or shared integration.

## Identity and scope

- Frozen root: `/tmp/peritus-parent-release-api-source-01`
- Full 65-file manifest SHA256: `e2ecbc4adf8307e78a18f2bbe312683cd2a03d76c6bc6145529f5266be07947f`
- Patch SHA256: `2dcf20ff3bc9e450589771931d38d9a9137bd18b4620d0d9db3de9ca4ee6940f`
- Parent report SHA256: `44d0ea4efb286337cebb91ca20f482c06651902fb72cfdf94e1be094db14df4e`
- Evidence manifest SHA256: `5fadf0e613498c64ad23b8accb615ee18471d0f0f744d4d25ade6188c3866067`
- Compared with `/tmp/peritus-sol-release-candidate-contracts-source-01`: the file lists are identical and exactly `src/error.rs` and `src/lib.rs` differ. The final two hashes match `/tmp/peritus-parent-release-api-changed-source.sha256`.
- All 65 frozen-source entries and all 19 evidence entries passed `sha256sum -c` from the roots matching each manifest's workspace-relative paths.

## Source findings

`ConstructionError::spec_code` is a total six-way mapping from the private stored `ConstructionErrorKind` to the six existing H4 literals. The production `const fn code` keeps its existing match and now ensures exact character-sequence equality to that mapping. `kind` already guarantees the exact stored category. There is no executable precondition, new runtime branch, alternate implementation, trust escape, or broadened lint suppression.

The negative patch changes only the runtime `ZeroRevision` arm to the `ZeroDigest` text while leaving `spec_code` unchanged. Its supplied strict run fails precisely at the new `code` postcondition with 376 verified / 1 error; the restored source passes 377 / 0. This is a meaningful correspondence control rather than a detached lemma test.

`lib.rs` exports only the existing `ready_evaluation_contract` and `release_inputs_ready` predicates under `cfg(verus_only)`. Ordinary Rust gets no new executable export. The actual `evaluate_release` production body already ensures `ready_evaluation_contract`; that predicate binds the candidate, evaluation time, all assessment sequences, canonical diagnostics, decision digest bytes, five component booleans, readiness iff the complete input predicate, and diagnostic emptiness iff readiness. The external verifier client imports these root exports and calls the actual evaluator without a requires clause, establishing that the new module boundary is usable by a downstream proof client. It separately calls the actual `code` and `kind` accessors for arbitrary errors and proves every literal.

I found no assumption, admit, axiom, external body, executable require, scanner exception, or cfg widening in the two-file patch or client. `spec_kind` remains a closed view of the private field; `spec_code` is public/open only in verification builds through the surrounding Verus surface.

## Evidence attribution and limits

I independently inspected the source, patch, client, qualifier JSON, negative patch/log, restored proof log, and all supplied hashes. I did not rerun the compiler. Parent-supplied evidence records strict library verification at 377/0, downstream-client verification at 3/0 using already-produced dependency metadata, the negative control at 376/1, 32 executable tests plus one compile-fail doctest, strict all-target Clippy, fmt, ordinary API scan (3513 files / 14689 entries), and layout scan (4414 files).

This increment proves exact diagnostic strings and exposes the already-proved input-relative evaluator contract to verifier clients. It does not prove observation truth, digest provenance or cryptographic authenticity, Git-object existence, platform truth, publication authority, all caller coverage, or hosted CI enforcement.
