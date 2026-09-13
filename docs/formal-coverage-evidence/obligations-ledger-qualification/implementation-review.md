# Peritus obligations performance and canonical encoding checkpoint

Verdict: ready for independent review. The 12-file isolated increment preserves public behavior,
error ordering, and the frozen 325-proof contracts while repairing a reproduced scaling regression
and proving the exact canonical bytes used by public ledger extraction.

## Runtime diagnosis

The temporary probe constructs two large alternative branches whose only unsatisfied member is
late in each branch. This avoids the early exits in the original 64/128/256 probe. Candidate325
took 61,685,344 / 475,554,183 / 3,693,194,776 / 29,112,903,862 ns for 256 / 512 / 1024 / 2048
entries, approximately 8x per doubling. The foundation source took 92,565 / 194,918 / 428,597 /
936,096 ns. Final fixed source took 99,878 / 218,495 / 456,405 / 1,014,757 ns. These are bounded
release-mode wall-clock observations on one synthetic fixture, not a universal complexity theorem.

The reproduced cause was the combination of linear ledger/evidence lookups and evaluating the
same incomplete alternative branch once for every member. The fix uses verified binary search for
ledger entries, condition observations, and requirement evidence, and records each checked branch
so each distinct branch is evaluated once. The existing exact lookup and complete-report contracts
remain unchanged. Evaluation can still be quadratic when the number of distinct branches grows
with the number of entries; this increment removes the demonstrated repeated-member cubic path.

## Canonical encoding proof

The executable encoder is now a verified kernel. Its postcondition equals an input-defined byte
sequence containing the domain separator, source digest, conversation revision, entry count, every
entry identity/clause/provenance field, every specification tag and payload, and every ordered path
identity/role/spelling. `RequirementLedger::extract` calls this exact encoder before hashing.

The permanent compatibility regression exercises all eleven specification variants, both schema
directions, every path role, binary clause/field bytes, and nontrivial integer encodings. The frozen
pre-proof serializer and the verified encoder both yield digest
`73ee3a1d6b4f33166b98bde48cf37f7faf90f3cffae776e9ed4f929ba206b8a4`.

SHA-256 execution remains ordinary Rust. No axiom, assumption, admit, external body, public
executable precondition, runtime rejection guard, or checker exception was added. This checkpoint
proves deterministic encoding supplied to SHA-256; it does not prove cryptographic authenticity,
collision resistance, or a mathematical SHA-256 implementation theorem.

## Evidence

- Full frozen source: `/tmp/peritus-sol-obligations-performance-source-01`
- Full 51-file manifest: `/tmp/peritus-sol-obligations-performance-full-package.sha256`
- Changed 12-file manifest: `/tmp/peritus-sol-obligations-performance-changed-source.sha256`
- Exact patch from frozen325: `/tmp/peritus-sol-obligations-performance-final.diff`
- Commands: `/tmp/peritus-sol-obligations-performance-commands-01.txt`
- Scaling comparison: `/tmp/peritus-sol-obligations-performance-scaling-comparison-02.json`
- Strict proof: `/tmp/peritus-sol-obligations-performance-canonical-verus-final-09.log`
- Default tests: `/tmp/peritus-sol-obligations-performance-tests-default-final-03.log`
- All-feature tests: `/tmp/peritus-sol-obligations-performance-tests-all-final-03.log`
- Clippy: `/tmp/peritus-sol-obligations-performance-clippy-final-06.log`
- Format: `/tmp/peritus-sol-obligations-performance-fmt-final-02.log`
- Layout: `/tmp/peritus-sol-obligations-performance-layout-final-02.log`
- API: `/tmp/peritus-sol-obligations-performance-api-final-02.log`

The strict proof emits only two pre-existing dependency warnings in
`peritus-run-settlement/src/evidence.rs`; the obligations package adds no Verus escape or warning.
