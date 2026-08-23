# peritus-protocol

`peritus-protocol` defines Peritus's stable, versioned domain wire messages. It maps canonical
frames to checked domain values without treating decoded bytes as authorization, evidence, or a
durable commit receipt.

Schema version 1 covers lifecycle commands, command envelopes, immutable event records, reducer
errors, lifecycle phases, B1 policy and budget data, and B2 acceptance contracts.
