# Candidate-specific release assets

## Folder guide

README describing externally generated candidate release assets; this source directory contains no release binaries.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

Actual artifacts are generated outside the source tree under a candidate-specific evidence root.
The retained layout has separate `artifacts/`, `sbom/`, `provenance/`, `signatures/`,
`qualification/`, `audit/`, and `documentation/` directories. Every regular file is represented by
path, byte length, media type, role, and SHA-256 in the H4 manifests.

No binary, signature, report, or audit stored here is considered evidence merely because it is
checked into Git. Release qualification admits only externally observed and authenticated bytes.
