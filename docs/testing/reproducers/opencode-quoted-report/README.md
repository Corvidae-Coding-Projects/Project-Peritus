# Literal quoted report replay

## Folder guide

Real-interface replay resources for opencode quoted report; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [expected.json](expected.json) | Structured expected data with fields `findings`, `summary`. |

This retained model-fidelity case uses an actual configured Zen / GPT-6 Luna route.
Open Peritus in a disposable folder, select OpenCode Zen through `peritus providers`,
then send this prompt with the exact contents of expected.json appended:

> Return exactly this JSON report as the public answer, preserving every field and string value:

Parse the public JSON and compare every field/string with expected.json, especially the
literal `purpose":"external_effect"` fragment. An additional backslash or missing closing
quote is a failure, even if the returned report remains valid JSON. Do not repair its semantic
values in a checker. Actual responses have both passed and failed exact copying; the bridge
can preserve the model's report object without guaranteeing model value fidelity.

The full public-consumer replay and original failed outputs are described in the OpenCode
real-testing report. This case contains no credential, fake provider, or unit-test substitute.
