#!/usr/bin/env python3
"""Operate the actual provider/developer loop and inspect real malformed-turn recovery."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--baseline", action="store_true")
parser.add_argument("--fault-count", type=int, default=1)
parser.add_argument("--arguments", default="{}")
parser.add_argument("--dsml", action="store_true")
parser.add_argument("--valid-empty", action="store_true", help="a declared no-argument skill_list call must remain valid")
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=False)
root = output / "workspace"
(root / "data/mods").mkdir(parents=True)
(root / "pack_lock.json").write_text('{"mods":["alpha.json","beta.json"]}\n')
(root / "data/mods/alpha.json").write_text('{"payload":"ALPHA-7391","structure":"fortress"}\n')
(root / "data/mods/beta.json").write_text('{"payload":"BETA-2846","structure":"village"}\n')
captures = output / "capture"
captures.mkdir()
wrapper = output / "actual-opencode"
shutil.copy2(Path(__file__).with_name("argument-fault-vendor.py"), wrapper)
wrapper.chmod(0o700)
environment = dict(os.environ)
environment.update({"PERITUS_REAL_CAPTURE": str(wrapper), "PERITUS_REAL_CAPTURE_DIR": str(captures),
    "PERITUS_REAL_VENDOR_BINARY": shutil.which("opencode") or "opencode", "PERITUS_ARGUMENT_FAULT_COUNT": str(args.fault_count)})
proposal = json.loads(args.arguments)
tool_name = "skill_list" if args.valid_empty else "workspace_read"
environment["PERITUS_ARGUMENT_FAULT_REPLY"] = json.dumps({"content":"I will inspect the pinned files.", "tool_calls":[{"name":tool_name, "arguments":proposal}]})
if args.dsml:
    if proposal:
        raise SystemExit("the live DSML case uses an empty argument set")
    environment["PERITUS_ARGUMENT_FAULT_REPLY"] = json.dumps('<tool_calls><invoke name="workspace_read"></invoke></tool_calls>')
spec = {"root":str(root), "state":str(output / "state"), "zen":True,
    "task":"Read pack_lock.json, inspect the exact files it pins under data/mods, and report each filename, payload identifier, and structure. Use read-only host tools. Finish this task; do not switch to an older task."}
(output / "spec.json").write_text(json.dumps(spec))
commands = [
    {"action":"batch", "prose":"The active task is to inspect these pinned files.", "calls":[["workspace_list", {"path":".", "depth":1}]]},
    {"action":"batch", "prose":"The manifest identifies the exact payloads; inspect them next.", "calls":[["workspace_read", {"path":"pack_lock.json"}]]},
    {"action":"live"},
]
(output / "commands.jsonl").write_text("".join(json.dumps(command) + "\n" for command in commands))
process = subprocess.run([str(args.binary.resolve()), str(output / "spec.json")], input=(output / "commands.jsonl").read_text(), text=True, capture_output=True, env=environment, timeout=600)
(output / "stdout.jsonl").write_text(process.stdout)
(output / "stderr.txt").write_text(process.stderr)
if process.returncode:
    raise SystemExit(f"consumer failed: {process.stderr[-1800:]}")
result = json.loads(process.stdout.splitlines()[-1])
blocks = [block for message in result["messages"] for block in message["blocks"]]
host_errors = [block for block in blocks if block.get("error")]
metas = [json.loads(path.read_text()) for path in captures.glob("*.meta.json")]
faulted = [path for path in captures.glob("*.meta.json") if json.loads(path.read_text())["faulted_instructions"]]
fault_seen = False
for meta in faulted:
    for line in meta.with_suffix("").with_suffix(".stdout").read_text().splitlines():
        event = json.loads(line)
        if event.get("type") == "text":
            if args.dsml:
                fault_seen |= event["part"]["text"].strip() == json.loads(environment["PERITUS_ARGUMENT_FAULT_REPLY"])
                continue
            try:
                reply = json.loads(event["part"]["text"])
            except (KeyError, ValueError):
                continue
            fault_seen |= any(call.get("name") == tool_name and call.get("arguments") == proposal for call in reply.get("tool_calls", []))
problems = []
observed_calls = [block for block in blocks if "call" in block and not block["call"].startswith("fixture_")]
if len(observed_calls) != result["tools"]:
    problems.append("returned history omitted live tool calls; cannot establish the host-error count")
if args.fault_count and not fault_seen:
    problems.append("the real model did not emit the requested bad call; correction branch unexercised")
if args.baseline:
    if not host_errors:
        problems.append("baseline did not expose an actual failed host tool call")
elif args.fault_count <= 1:
    if host_errors:
        problems.append("a malformed read reached the host instead of being corrected")
    if args.fault_count and not args.valid_empty and not any(meta["correction"] for meta in metas):
        problems.append("no actual correction inference was observed")
    if args.fault_count and not args.valid_empty and not any("Argument correction required: workspace_read" in path.read_text() for path in captures.glob("*.original-input")):
        problems.append("the required-argument correction branch was not exercised")
    if args.valid_empty and any("Argument correction required:" in path.read_text() for path in captures.glob("*.original-input")):
        problems.append("a valid empty argument object was unnecessarily corrected")
else:
    if len(faulted) != args.fault_count or not host_errors or not any(meta["correction"] for meta in metas):
        problems.append("repeated-invalid-call fallback did not retain actionable host rejection")
for expected in ("alpha.json", "beta.json", "ALPHA-7391", "BETA-2846", "fortress", "village"):
    if expected not in result["reply"]:
        problems.append(f"final report missing {expected}")
summary = {"status":"FAIL" if problems else "PASS", "provider_requests":len(metas), "correction_requests":sum(meta["correction"] for meta in metas),
    "host_errors":len(host_errors), "model_turns":result["turns"], "tool_calls":result["tools"], "fault_seen":fault_seen, "problems":problems}
(output / "results.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary))
raise SystemExit(bool(problems))
