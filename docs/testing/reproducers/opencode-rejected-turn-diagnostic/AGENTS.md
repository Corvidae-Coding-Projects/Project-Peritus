# docs/testing/reproducers/opencode-rejected-turn-diagnostic — navigation

## Responsibility

Scripted-vendor replay resources for OpenCode turn refusal: retained rejected text, the private-trace
reader, and the developer loop's refusal/retry narration; start with the local README and its
documented sandbox-free prerequisite.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide, expectations table, and operating/limit notes for this folder; AGENTS.md provides the local navigation map. |
| [consumer.rs](consumer.rs) | Provider-level consumer over the production registry and scripted stand-in; checks the bounded correction extension and terminal outcome. |
| [agent.rs](agent.rs) | Developer-loop consumer with a recording interaction; counts the public activities one refused turn publishes. |
| [scripted-opencode.py](scripted-opencode.py) | Scripted OpenCode stand-in that answers the adapter's `--version`, `models`, and `run` commands from a fixed reply script. |
| [replay.py](replay.py) | Builds both consumers offline and runs the ten scenarios with disposable configuration, key, reply script, and prompt capture. |
| [observed-baseline.txt](observed-baseline.txt) | Verbatim transcripts of the recorded run and its check outcomes. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
