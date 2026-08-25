# peritus-collaboration

`peritus-collaboration` is the durable, deterministic D3 causal-delegation aggregate. It owns
root/parent/depth task structure, offered-to-active ownership, bounded messages, joins, exact
artifact handoffs, pause, cancellation propagation, terminal truth, replay, and C0 persistence.

The crate consumes scheduler identities and exact reservation observations but never schedules or
executes work. Messages and assignments are inert data, not authority. Every accepted command
produces exactly one event and one complete successor checkpoint; restart is reconstructed from
canonical family 74 events and checked against the family 75 state checkpoint.

Canonical schema-v1 families are 73 (commands), 74 (events), and 75 (state). The C0 checkpoint
namespace is `0xD302` and the aggregate kind is `Collaboration`.
