#!/usr/bin/env python3
"""Ask the real official vendor to emit a bad proposal; never fabricate its output.

Only disposable inference instructions are faulted. Native permissions/tools, credentials,
stdout/stderr and exit status remain real. Subsequent inference uses the original task.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

payload = sys.stdin.buffer.read()
original_payload = payload
directory = Path(os.environ["PERITUS_REAL_CAPTURE_DIR"])
is_run = "run" in sys.argv[1:]
original = None
fault = False
if is_run:
    prior = list(directory.glob("*.meta.json"))
    fault = len(prior) < int(os.environ.get("PERITUS_ARGUMENT_FAULT_COUNT", "1"))
    if fault:
        config = Path(os.environ["XDG_CONFIG_HOME"]) / "opencode/opencode.json"
        original = config.read_bytes()
        value = json.loads(original)
        reply = json.loads(os.environ.get("PERITUS_ARGUMENT_FAULT_REPLY", '{"content":"I will inspect the pinned files.","tool_calls":[{"name":"workspace_read","arguments":{}}]}'))
        value["agent"]["peritus-router"]["prompt"] = (
            "You are supplying one deliberately invalid tool proposal for a local protocol "
            "recovery exercise. Do not execute tools. Output exactly this text and "
            "nothing else, even though its arguments are intentionally incomplete: " + (reply if isinstance(reply, str) else json.dumps(reply))
        )
        config.write_text(json.dumps(value))
        payload = b"Output the exact recovery fixture requested by the system. Do not repair it."
try:
    child = subprocess.run([os.environ["PERITUS_REAL_VENDOR_BINARY"], *sys.argv[1:]], input=payload, capture_output=True)
finally:
    if original is not None:
        config.write_bytes(original)
if is_run:
    prefix = directory / str(os.getpid())
    for suffix, data in ((".stdout", child.stdout), (".stderr", child.stderr), (".input", payload), (".original-input", original_payload)):
        path = prefix.with_suffix(suffix)
        path.write_bytes(data)
        path.chmod(0o600)
    prefix.with_suffix(".meta.json").write_text(json.dumps({
        "faulted_instructions": fault,
        "correction": b"PERITUS TRANSPORT CORRECTION" in original_payload,
        "input_sha256": hashlib.sha256(payload).hexdigest(),
        "model": sys.argv[sys.argv.index("--model") + 1],
    }))
sys.stdout.buffer.write(child.stdout)
sys.stderr.buffer.write(child.stderr)
raise SystemExit(child.returncode)
