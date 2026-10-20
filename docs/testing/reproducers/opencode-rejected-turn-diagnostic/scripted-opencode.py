#!/usr/bin/env python3
"""Scripted OpenCode stand-in for the rejected-turn diagnostic replay.

The adapter issues three vendor commands: `--version`, `models <service> --verbose`, and
`run ... --model <service>/<model>`. This script answers all three from a fixed reply script
read from `PERITUS_SCRIPT`; each `run` consumes the next entry through the counter file named
by `PERITUS_SCRIPT_STATE`, so the two vendor processes of one corrected turn see the intended
sequence. It performs no network access, reads no credential, and never runs vendor code.

Reply entries are `valid` (one complete private envelope), `invalid` (the same envelope with one
extra closing brace), `prelude_backticks` (plain prose with an inline code span, a blank line, then
the envelope), `prelude_quoted` (quoted pseudo-tool markup, a blank line, then the envelope),
`prelude_fenced` (plain prose, a blank line, then the envelope inside a `json` fence), `dsml`
(a prose prelude followed by a complete DeepSeek DSML calls block with one standard call and one
nested arguments call), `dsml_unknown_tool` (the same block naming an undeclared tool),
`dsml_malformed` (a parameter without its closing tag), or `rate_limit` (a 429 vendor error frame).
The extra brace is the defect retained from the real captured failure: the bounded repair extracts
only a container that ends the payload, so this reply is refused. The three prelude forms and the
DSML forms are the repair shapes observed in the real run.
"""
import json
import os
import sys

MODEL = "scripted-reply-model"
SESSION = "ses_scripted_rejected_turn"
MESSAGE = "msg_scripted_rejected_turn"
ENVELOPE = '{"content":"Scripted reply for the rejected-turn diagnostic replay.","tool_calls":[]}'
PRELUDE_BACKTICKS = "Baseline is green at HEAD `ba6c0db`.\n\n"
PRELUDE_QUOTED = (
    'Announced <tool_call name="patch" path="src/main.rs">\n'
    '<parameter name="content">\n\n'
)
PRELUDE_FENCED = "Baseline is green at HEAD ba6c0db.\n\n```json\n"
DSML = (
    "I will inspect the README and search the source.\n\n"
    '<｜｜DSML｜｜ calls>\n'
    '<｜｜DSML｜｜ invoke name="workspace_read">\n'
    '<｜｜DSML｜｜ parameter name="path" string="true">README.md</｜｜DSML｜｜ parameter>\n'
    "</｜｜DSML｜｜ invoke>\n"
    '<｜｜DSML｜｜ invoke name="workspace_search">\n'
    '<｜｜DSML｜｜ parameter name="arguments" string="false">{"query":"TODO","path":"src"}</｜｜DSML｜｜ parameter>\n'
    '<｜｜DSML｜｜ parameter name="name" string="true">workspace_search</｜｜DSML｜｜ parameter>\n'
    "</｜｜DSML｜｜ invoke>\n"
    "</｜｜DSML｜｜ calls>"
)
DSML_UNKNOWN_TOOL = DSML.replace('name="workspace_read"', 'name="workspace_delete"')
DSML_MALFORMED = (
    "I will read the README.\n\n"
    '<｜｜DSML｜｜ calls>\n'
    '<｜｜DSML｜｜ invoke name="workspace_read">\n'
    '<｜｜DSML｜｜ parameter name="path" string="true">README.md\n'
    "</｜｜DSML｜｜ invoke>\n"
    "</｜｜DSML｜｜ calls>"
)


def reply_text(reply: str) -> str:
    if reply == "valid":
        return ENVELOPE
    if reply == "invalid":
        return ENVELOPE + "}"
    if reply == "prelude_backticks":
        return PRELUDE_BACKTICKS + ENVELOPE
    if reply == "prelude_quoted":
        return PRELUDE_QUOTED + ENVELOPE
    if reply == "prelude_fenced":
        return PRELUDE_FENCED + ENVELOPE + "\n```"
    if reply == "dsml":
        return DSML
    if reply == "dsml_unknown_tool":
        return DSML_UNKNOWN_TOOL
    if reply == "dsml_malformed":
        return DSML_MALFORMED
    raise ValueError(f"unknown scripted reply {reply}")


def main() -> int:
    arguments = sys.argv[1:]
    if "--version" in arguments:
        print("1.19.0")
        return 0
    if "models" in arguments:
        service = arguments[arguments.index("models") + 1]
        print(f"{service}/{MODEL}")
        print(
            json.dumps(
                {
                    "id": MODEL,
                    "providerID": service,
                    "name": "Scripted reply model",
                    "capabilities": {"toolcall": True},
                    "limit": {"context": 32768, "output": 4096},
                }
            )
        )
        return 0
    if "run" in arguments:
        # Replies come only from the script; the prompt is optionally retained for replay checks.
        prompt = sys.stdin.buffer.read()
        prompt_capture = os.environ.get("PERITUS_SCRIPT_PROMPT")
        if prompt_capture:
            with open(prompt_capture, "wb") as handle:
                handle.write(prompt)
        reply = next_reply(
            os.environ["PERITUS_SCRIPT_STATE"], os.environ["PERITUS_SCRIPT"]
        )
        if reply == "rate_limit":
            emit_error(429, "scripted rate limit")
        else:
            emit(reply_text(reply))
        return 0
    return 2


def next_reply(state_path: str, script_path: str) -> str:
    with open(script_path, encoding="utf-8") as handle:
        script = json.load(handle)
    index = 0
    if os.path.exists(state_path):
        with open(state_path, encoding="utf-8") as handle:
            index = int(handle.read().strip() or "0")
    reply = script[min(index, len(script) - 1)]
    with open(state_path, "w", encoding="utf-8") as handle:
        handle.write(str(index + 1))
    return reply


def emit(text: str) -> None:
    rows = [
        {
            "type": "step_start",
            "timestamp": 1,
            "sessionID": SESSION,
            "part": {
                "id": "prt_scripted_start",
                "messageID": MESSAGE,
                "sessionID": SESSION,
                "type": "step-start",
            },
        },
        {
            "type": "text",
            "timestamp": 2,
            "sessionID": SESSION,
            "part": {
                "id": "prt_scripted_text",
                "messageID": MESSAGE,
                "sessionID": SESSION,
                "type": "text",
                "text": text,
            },
        },
        {
            "type": "step_finish",
            "timestamp": 3,
            "sessionID": SESSION,
            "part": {
                "id": "prt_scripted_finish",
                "reason": "stop",
                "messageID": MESSAGE,
                "sessionID": SESSION,
                "type": "step-finish",
                "tokens": {
                    "total": 12,
                    "input": 9,
                    "output": 3,
                    "reasoning": 0,
                    "cache": {"write": 0, "read": 0},
                },
            },
        },
    ]
    for row in rows:
        print(json.dumps(row))


def emit_error(status: int, message: str) -> None:
    print(
        json.dumps(
            {
                "type": "error",
                "timestamp": 4,
                "sessionID": SESSION,
                "error": {
                    "name": "AI_APICallError",
                    "data": {"statusCode": status, "message": message},
                },
            }
        )
    )


if __name__ == "__main__":
    raise SystemExit(main())
