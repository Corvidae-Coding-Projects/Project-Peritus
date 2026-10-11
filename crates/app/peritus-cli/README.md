# peritus-cli

G1 owns the scriptable `peritus` command-line client. The crate connects only to the protected
local G0 endpoint and expresses every operation through the A3 application protocol. It provides
strict argument parsing, stable JSON and human output, stable exit categories, resumable event and
artifact streams, prompt responses, terminal control, and shell completions.

The `runs` command family lists and inspects durable coding runs and can continue, execute, export,
accept, commit, discard, retry, or cancel one exact run. Human and JSON output include settlement
cause, candidate digest, qualification evidence, changed paths, successful checks, and run
instructions. Accepting or committing an unqualified candidate requires repeating the command with
the exact digest printed by the client.

`peritus --endpoint <address> shutdown --wait` retains the authenticated connection while G0 sends
six bounded progress observations and one correlated clean or unclean completion. It no longer
mistakes orderly endpoint withdrawal for an I/O failure.

The client never grants authority or infers durable success. G0 authenticates the peer and session,
checks current authority, commits commands, and reports typed outcomes. This crate depends only on
the application protocol, canonical codec, foundational types, and its local async transport.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-cli
```
