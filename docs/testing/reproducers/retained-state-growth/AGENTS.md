# Retained-state growth replay navigation

Real-interface regression resources. These are external consumers and native-operation drivers,
not Cargo workspace tests. Start with README.md and its disposable-state requirements.

| File | Role |
| --- | --- |
| README.md | Commands, evidence boundaries, and safe fixture ownership. |
| build_consumer.py | Builds public consumers outside the checkout with two Cargo workers. |
| consumer.rs | Exact working-state migration, growth, page ordering and persistence consumer. |
| protocol_consumer.rs | Actual message/event codecs, large JSON and reducer metadata consumer. |
| host_consumer.rs | Production context recovery, provider serialization, checkpoint and inspection consumer. |
| scope_growth.py | Drives actual scoped-baseline consumer with large, deep and preview-heavy trees. |
| prepare_trace.py | Prepares a valid trace over 1 GiB using a closed disposable native fixture. |
| prepare_native.py | Copies a closed completed fixture and pads its valid run metadata. |
| native_replay.py | Checks native launcher/TUI reattachment and actual runs-show readback. |
| prepare_memory_chat.py | Creates forty sources and a 33-operation memory task in a fresh project. |
| memory_chat_replay.py | Drives a real provider conversation and checks exact memory-tool trace evidence. |
| fixtures/ | Compressed original canonical snapshot inputs and exact-byte manifest. |

Parent: [Reproducers](../AGENTS.md). Implementation: working C6 codecs, local_context, and workspace_delivery.
