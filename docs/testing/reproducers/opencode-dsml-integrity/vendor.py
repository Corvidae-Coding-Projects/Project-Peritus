#!/usr/bin/env python3
"""External CLI wire fixture. No network, real credential access, inference, or tool execution."""
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
model = "scripted-reply-model"
if "--version" in args:
    print("1.19.0")
elif "models" in args:
    service = args[args.index("models") + 1]
    print(f"{service}/{model}")
    print(json.dumps({"id": model, "providerID": service, "name": "Wire fixture", "capabilities": {"toolcall": True}, "limit": {"context": 32768, "output": 4096}}))
elif "run" in args:
    sys.stdin.buffer.read()
    case_path = Path(os.environ["PERITUS_DSML_CASE"])
    case = json.loads(case_path.read_text())
    counter = case_path.with_suffix(".counter")
    index = int(counter.read_text()) if counter.exists() else 0
    counter.write_text(str(index + 1))
    text = case["reply"] if index == 0 or case.get("repeat_rejected") else json.dumps({"content": "Recovered safely.", "tool_calls": []})
    session, message = "ses_dsml_integrity", "msg_dsml_integrity"
    for kind, part in [
        ("step_start", {"type": "step-start"}),
        ("text", {"type": "text", "text": text}),
        ("step_finish", {"type": "step-finish", "reason": "stop", "tokens": {"input": 9, "output": 3, "reasoning": 0, "total": 12}}),
    ]:
        part.update({"sessionID": session, "messageID": message})
        print(json.dumps({"type": kind, "sessionID": session, "part": part}))
else:
    raise SystemExit(2)
