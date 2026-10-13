# Release metadata updater regression

The published 0.0.7 GitHub release has 52 assets. Its 110,283-byte metadata response
exceeded the launcher's 64 KiB bound, so the actual `peritus update` command failed
before comparing versions. The published 0.0.7 binary independently reproduces this
failure. The prior implementation also buffered the entire response before applying
its bound.

The corrected contract accepts valid metadata through 4 MiB and rejects larger
responses before retaining bytes beyond that limit. It must enforce this for both
declared lengths and chunked responses. Unknown metadata fields and Unicode remain
valid; malformed JSON, absent/non-string tags, invalid versions, unsuccessful HTTP
statuses, and truncated responses must remain explicit errors. Rejection must not
start an installer. A current release must report that the installation is current.

Build the real CLI and operate it:

```sh
cargo build --locked --release -p peritus-cli -j 2
python3 docs/testing/reproducers/updater-release-metadata/replay.py \
  --binary target/release/peritus \
  --baseline /path/to/published-0.0.7/bin/peritus \
  --live --output /tmp/peritus-update-results.json
```

The replay starts a loopback HTTPS endpoint and invokes `peritus update` in a fresh
pseudoterminal for each scenario. A temporary CA and proxy are scoped to those
processes; system trust, application code, credentials, and user state are untouched.
The endpoint verifies the real updater's requested GitHub URL. Responses are
ordinary release metadata inputs, not a replacement updater. Sizes straddle the
old and new limits, and seed 424242 varies transport chunk sizes. XDG application
directories are disposable. The baseline option preserves a concrete 65,537-byte
failure replay even if GitHub's live response changes.

The `--live` scenario additionally operates the unchanged public GitHub URL without
the fixture proxy. It expects public version 0.0.7; update that expectation explicitly
when the public release changes. This replay covers release discovery and rejection,
not native installation, daemon recovery, or every updater behavior.

## Observed replay, October 12, 2026

`cargo build --locked --release -p peritus-cli -j 2` produced the optimized candidate
in 1 minute 49 seconds. Its SHA-256 was
`c8da9a6e43b0b719f0b7a4d167a124d3eb4691180c1a81a596eea6ab61122707`.
The replay above completed 29 scenarios with seed 424242: the published binary
reproduced the original rejection; all 27 candidate fixture scenarios passed;
and the public GitHub request reported `Peritus 0.0.7 is already current.`

Content-length and chunked responses passed at 65,535, 65,536, 65,537, 65,538,
110,283, 262,144, 1,048,576, 4,194,303, and 4,194,304 bytes. Declared and chunked
responses above 4 MiB failed explicitly. Malformed JSON, omitted/non-string tags,
invalid versions, HTTP 404/500, and truncated bodies retained their rejection paths.
Every fixture scenario observed exactly one request to the release-discovery URL.
No unit tests or repository/formal verification suites were run.

The published native package was installed through `Upgrade-Peritus.sh`, with its
SHA-256 matching the release sidecar and previously recorded release digest. Both
installed commands reported 0.0.7. The tested candidate CLI was then installed
atomically, retaining the published 0.0.7 daemon and a backup of the published CLI.
A fresh invocation of the installed `peritus update` against public GitHub exited 0
and reported that 0.0.7 was current. The existing browser run stayed interrupted and
recoverable; this update did not restart it.

Machine-local evidence is under `/home/doll/Peritus-updater-2026-10-12`, including
`replay-results.json`, `installation.json`, the original GitHub response, the saved
browser run record, and prior installed binaries.

After adding the folder-reopen fix, all 29 updater scenarios were replayed against
the final installed CLI, SHA-256
`a8e88eec12131f29264037f9287a71938e2e8e5d3f5ab824f1589010698d428a`.
They all passed, including public GitHub discovery. The final replay report is
`/home/doll/Peritus-session-recovery-2026-10-12/final-updater-results.json`.
