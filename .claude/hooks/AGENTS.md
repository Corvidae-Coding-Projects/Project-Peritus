# .claude/hooks — navigation

## Responsibility

Claude event-hook implementations and their local configuration helper.

## Files in this folder

| File | Role |
| --- | --- |
| [chainlink_config.py](chainlink_config.py) | Shared configuration and utilities for chainlink hooks. Imported by other hook scripts to avoid code duplication. |
| [pre-web-check.py](pre-web-check.py) | Chainlink web security hook for Claude Code. Injects RFIP (Recursive Framing Interdiction Protocol) before web tool calls. Triggered by PreToolUse on WebFetch\\|WebSearch to defend against prompt injection. |
| [prompt-guard.py](prompt-guard.py) | Chainlink behavioral hook for Claude Code. Injects best practice reminders on every prompt submission. Loads rules from .chainlink/rules/ markdown files. |
| [session-start.py](session-start.py) | Session start hook that loads chainlink context and auto-starts sessions. |
| [work-check.py](work-check.py) | PreToolUse hook that blocks Write\\|Edit\\|Bash unless a chainlink issue is being actively worked on. Forces issue creation before code changes. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
