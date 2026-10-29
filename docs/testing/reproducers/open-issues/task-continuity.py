#!/usr/bin/env python3
"""Drive real #160 tool/context operations and check task continuity independently."""
import argparse
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--live", action="store_true")
parser.add_argument("--zen", action="store_true")
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=False)
task = (
    "Earlier task: confirm the build identity; that task is finished. Current task: inspect "
    "pack_lock.json, then read the exact files it pins under data/mods and report each pinned "
    "name, payload identifier, and its structure. Do not rerun build-identity checks. "
    "This is read-only; finish inspection using the host filesystem tools."
)
cases = [
    ("missing-path", {}), ("null-path", {"path": None}),
    ("numeric-path", {"path": 17}), ("bool-path", {"path": False}),
    ("array-path", {"path": ["pack_lock.json"]}),
    ("object-path", {"path": {"value": "pack_lock.json"}}),
    ("misspelled-path", {"file": "pack_lock.json"}),
    ("empty-path", {"path": ""}),
    ("bad-range-type", {"path": "pack_lock.json", "start_line": "one"}),
    ("missing-file", {"path": "absent.json"}),
    ("traversal", {"path": "../private-outside.txt"}),
    ("denied-file", {"path": "private.txt"}),
]
if args.live:
    cases = [("live-four-missing-path", {})]
results = []


def batch(prose, calls):
    return {"action": "batch", "prose": prose, "calls": calls}


def blocks(view):
    return [block for message in view["messages"] for block in message["blocks"]]


def check_history(view, plan, count, sibling):
    problems = []
    assistant = [m for m in view["messages"] if m["role"] == "Assistant"]
    if not any(plan == b.get("text") for m in assistant for b in m["blocks"]):
        problems.append("current plan missing from its assistant exchange")
    rejected = [b for b in blocks(view) if b.get("error") and "fixture_3_" in b.get("result", "")]
    if len(rejected) != count:
        problems.append(f"expected {count} current rejections, received {len(rejected)}")
    calls = {b["call"] for b in blocks(view) if "call" in b}
    receipts = [b["result"] for b in blocks(view) if "result" in b]
    if len(receipts) != len(set(receipts)) or calls != set(receipts):
        problems.append("tool proposal/result pairing lost")
    if sibling and not any("PUBLIC-SIBLING-160" in b.get("output", "") for b in blocks(view)):
        problems.append("successful sibling read disappeared")
    if "PRIVATE-CONTENT-160" in json.dumps(view):
        problems.append("protected source content leaked")
    return problems


for name, invalid in cases:
    for sibling in ([False] if args.live else [False, True]):
        case = output / (name + ("-mixed" if sibling else "-only"))
        root = case / "workspace"
        (root / "data/mods").mkdir(parents=True)
        (root / "pack_lock.json").write_text(json.dumps({"mods": ["alpha.json", "beta.json"]}) + "\n")
        (root / "data/mods/alpha.json").write_text('{"payload":"ALPHA-7391","structure":"fortress"}\n')
        (root / "data/mods/beta.json").write_text('{"payload":"BETA-2846","structure":"village"}\n')
        (root / "identity.txt").write_text("OLD-TASK-COMPLETE-160\n")
        (root / "public.txt").write_text("PUBLIC-SIBLING-160\n")
        (root / "private.txt").write_text("PRIVATE-CONTENT-160\n")
        (case / "private-outside.txt").write_text("OUTSIDE-160\n")
        spec = {"root": str(root), "state": str(case / "state"), "task": task, "zen": args.zen}
        (case / "spec.json").write_text(json.dumps(spec))
        plan = "CURRENT-PLAN-160: inspect the pinned mod payloads next; build identity is complete."
        count = 4 if args.live or name == "missing-path" else 1
        calls = [["workspace_read", invalid] for _ in range(count)]
        if sibling:
            calls.insert(0, ["workspace_read", {"path": "public.txt"}])
        commands = [
            {"action": "protect", "paths": ["private.txt"]},
            batch("OLDER-TASK-160: build identity was confirmed.", [["workspace_read", {"path": "identity.txt"}]]),
            {"action": "view"},
            batch(plan, calls),
            {"action": "view"},
        ]
        if args.live:
            commands.append({"action": "live"})
        else:
            commands.extend([
                {"action": "reopen"}, {"action": "view"},
                batch("CURRENT-CORRECTED-160: inspect the manifest with a valid path.", [["workspace_read", {"path": "pack_lock.json"}]]),
                {"action": "view"},
                {"action": "deny", "value": True}, {"action": "view"},
                {"action": "deny", "value": False}, {"action": "view"},
            ])
        (case / "commands.jsonl").write_text("".join(json.dumps(c) + "\n" for c in commands))
        process = subprocess.run([str(args.binary.resolve()), str(case / "spec.json")], input=(case / "commands.jsonl").read_text(), text=True, capture_output=True, timeout=600)
        (case / "stdout.jsonl").write_text(process.stdout)
        (case / "stderr.txt").write_text(process.stderr)
        problems = []
        if process.returncode:
            problems.append(f"consumer exit {process.returncode}: {process.stderr[-1600:]}")
        else:
            rows = [json.loads(line) for line in process.stdout.splitlines()]
            problems.extend(check_history(rows[4], plan, count, sibling))
            if args.live:
                live = rows[5]
                reply = live["reply"]
                for expected in ("alpha.json", "beta.json", "ALPHA-7391", "BETA-2846", "fortress", "village"):
                    if expected not in reply:
                        problems.append(f"live report missing observed {expected}")
                inspected = {json.loads(b["arguments"]).get("path") for b in blocks(live) if b.get("name") == "workspace_read"}
                if not {"pack_lock.json", "data/mods/alpha.json", "data/mods/beta.json"}.issubset(inspected):
                    problems.append("live recovery did not inspect all current-task files")
            else:
                problems.extend(check_history(rows[6], plan, count, sibling))
                corrected = [b for b in blocks(rows[8]) if b.get("result") == "fixture_7_0"]
                if len(corrected) != 1 or corrected[0].get("error") or "alpha.json" not in corrected[0].get("output", ""):
                    problems.append("corrected retry lost its successful observation")
                if "OLD-TASK-COMPLETE-160" in json.dumps(rows[10]) or "PUBLIC-SIBLING-160" in json.dumps(rows[10]):
                    problems.append("read revocation exposed previously read source bytes")
                if "PRIVATE-CONTENT-160" in json.dumps(rows):
                    problems.append("protected source content leaked")
                # After a later successful step and permission change, older exchanges may
                # legitimately roll into the archive. The corrected plan is now current.
                restored = rows[12]
                if not any("CURRENT-CORRECTED-160" in b.get("text", "") for m in restored["messages"] if m["role"] == "Assistant" for b in m["blocks"]):
                    problems.append("permission restoration lost the current corrected plan")
                if not any(b.get("result") == "fixture_7_0" and not b.get("error") for b in blocks(restored)):
                    problems.append("permission restoration lost the corrected result")
        results.append({"case": case.name, "status": "FAIL" if problems else "PASS", "problems": problems})
        print(json.dumps(results[-1]), flush=True)
(output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
raise SystemExit(any(result["status"] == "FAIL" for result in results))
