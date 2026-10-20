# OpenCode DSML integrity replay — navigation

This folder preserves model-reply failures through the real public provider adapter. The external
vendor is an explicitly identified wire fixture, not live inference. No tool payload is executed.

| File | Role |
| --- | --- |
| [README.md](README.md) | Scope, independent expected behavior, build/replay commands and evidence links. |
| [consumer.rs](consumer.rs) | Public provider registry, request, correction, event/reducer consumer and canonical trace capture. |
| [vendor.py](vendor.py) | External vendor CLI wire fixture, with catalog/version responses and one safe correction. |
| [replay.py](replay.py) | Concrete escaped-defect inputs and seeded varied literals, exact inert-call and rejection checks. |
| [link.py](link.py) | Links either public consumer from the actual Cargo JSON artifact cohort. |

[Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).
