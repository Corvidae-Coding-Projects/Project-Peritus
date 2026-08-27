# peritus-cli

G1 owns the scriptable `peritus` command-line client. The crate connects only to the protected
local G0 endpoint and expresses every operation through the A3 application protocol. It provides
strict argument parsing, stable JSON and human output, stable exit categories, resumable event and
artifact streams, prompt responses, terminal control, and shell completions.

The client never grants authority or infers durable success. G0 authenticates the peer and session,
checks current authority, commits commands, and reports typed outcomes. This crate depends only on
the application protocol, canonical codec, foundational types, and its local async transport.
