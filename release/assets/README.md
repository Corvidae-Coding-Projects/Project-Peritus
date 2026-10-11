# Candidate-specific release assets

Actual artifacts are generated outside the source tree under a candidate-specific evidence root.
The retained layout has separate `artifacts/`, `sbom/`, `provenance/`, `signatures/`,
`qualification/`, `audit/`, and `documentation/` directories. Every regular file is represented by
path, byte length, media type, role, and SHA-256 in the H4 manifests.

No binary, signature, report, or audit stored here is considered evidence merely because it is
checked into Git. Release qualification admits only externally observed and authenticated bytes.
