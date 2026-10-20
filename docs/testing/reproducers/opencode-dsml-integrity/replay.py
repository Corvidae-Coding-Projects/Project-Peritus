#!/usr/bin/env python3
"""Operate the public provider adapter with exact bad model replies and generated literal values.

The external vendor is a wire fixture, not live inference. The parser, correction path,
permission-free inert call publication, and response reducer are the production implementation.
"""
import argparse
import json
import os
from pathlib import Path
import random
import shutil
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--consumer", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
root = a.output.resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)
(root / "tmp").mkdir(mode=0o700)
vendor = root / "opencode-wire-fixture"
shutil.copy2(Path(__file__).with_name("vendor.py"), vendor)
vendor.chmod(0o700)
cases = []

def block(arguments, marker="", root_tag="calls", name="workspace_write"):
    def tag(name, closing=False):
        return "<" + ("/" if closing else "") + marker + name + ">"
    body = tag(root_tag) + tag('invoke name="' + name + '"')
    for key, value, string in arguments:
        body += tag('parameter name="' + key + '" string="' + str(string).lower() + '"')
        body += value + tag("parameter", True)
    return body + tag("invoke", True) + tag(root_tag, True)

def add(name, reply, calls=None, **options):
    cases.append(dict(name=name, reply=reply, expected_calls=calls, **options))

call = lambda text: [{"name": "workspace_write", "arguments": {"path": "out.txt", "content": text}}]
literals = [
    "literal <｜｜DSML｜｜ example>",
    "literal </|｜DSML｜| example>\n猫 🐦 quoted \"text\" \\",
    '<calls>literal </calls> <invoke name="example">literal </invoke>',
    "{\"content\":\"data\",\"tool_calls\":[]} <tool_calls> literal </tool_calls>",
    "  \n\t", "",
]
markers = ["", "｜｜DSML｜｜", "｜｜DSML｜｜ ", "|｜DSML｜|", "|｜DSML｜| "]
for i, marker in enumerate(markers):
    for root_tag in ["calls", "tool_calls"]:
        for j, text in enumerate(literals):
            add(f"literal-{i}-{root_tag}-{j}", block([("path", "out.txt", True), ("content", text, True)], marker, root_tag), call(text))

safe = block([("path", "target.txt", True)], name="workspace_remove")
for i, prefix in enumerate([
    '{"content":"Finished.","tool_calls":[]}\n\n',
    'An earlier payload:\n{"content":"Finished.","tool_calls":[]}\n\n',
    '```json\n{"content":"Finished.","tool_calls":[]}\n```\n',
    '[{"content":"Finished.","tool_calls":[]}]\n',
]):
    add(f"competing-{i}", prefix + safe)

for i, value in enumerate([
    '{"path":"first.txt","path":"second.txt"}',
    '{"nested":{"x":1,"x":2}}',
    '[{"x":1,"x":2}]',
    '{"pa\\u0074h":"first.txt","path":"second.txt"}',
]):
    add(f"duplicate-json-{i}", block([("name", "workspace_read", True), ("arguments", value, False)], name="workspace_read"))

typed = {"path": "猫.txt", "options": {"values": [1, True, None], "text": literals[0]}}
add("typed-nested", block([("name", "workspace_read", True), ("arguments", json.dumps(typed), False)], name="workspace_read"), [{"name": "workspace_read", "arguments": typed}])
add("plain-prose", "Inspecting the requested document.\n\n" + block([("path", "design.md", True)], name="workspace_read"), [{"name": "workspace_read", "arguments": {"path": "design.md"}}])
for name, reply in [
    ("duplicate-parameter", block([("path", "first.txt", True), ("path", "second.txt", True)], name="workspace_read")),
    ("truncated", safe[:-8]),
    ("two-blocks", safe + safe),
    ("trailing-prose", safe + " extra output"),
    ("unknown-tool", block([], name="workspace_delete")),
    ("wrong-close-style", safe.replace("</invoke>", "</｜｜DSML｜｜invoke>")),
]:
    add(name, reply)
add("persistent-refusal", safe + " extra output", repeat_rejected=True)
add("tools-disabled", safe, choice="none")
add("specific-tool", safe, choice="specific", repeat_rejected=True)
read = block([("path", "design.md", True)], name="workspace_read")
search = block([("query", "navigation", True)], name="workspace_search")
two = read.removesuffix("</calls>") + search.removeprefix("<calls>")
add("two-calls", two, [
    {"name": "workspace_read", "arguments": {"path": "design.md"}},
    {"name": "workspace_search", "arguments": {"query": "navigation"}},
])
add("call-count", two.removesuffix("</calls>") + read.removeprefix("<calls>"))

json_reply = json.dumps({"content": "", "tool_calls": call(literals[0])}, ensure_ascii=False)
add("ordinary-json", json_reply, call(literals[0]), normalization=False)
add("json-quoted-prelude", 'Announced <tool_call name="example">\n\n' + json_reply, call(literals[0]), normalization=False)
add("go-route", block([("path", "out.txt", True), ("content", literals[1], True)], "｜｜DSML｜｜ "), call(literals[1]), service="opencode-go")

rng = random.Random(20261020)
for i in range(24):
    text = "".join(rng.choice(literals[:4] + ["\n", " ", "猫", "&lt;literal&gt;"]) for _ in range(rng.randrange(1, 5)))
    marker = rng.choice(markers)
    add(f"generated-{i}", block([("content", text, True), ("path", "out.txt", True)], marker, rng.choice(["calls", "tool_calls"])), call(text))

results = []
for i, case in enumerate(cases):
    path = root / f"{i:03d}-{case['name']}.json"
    path.write_text(json.dumps(case, ensure_ascii=False))
    env = dict(os.environ, PERITUS_DSML_CASE=str(path), PERITUS_DSML_VENDOR=str(vendor),
               PERITUS_DSML_TRACE=str(path.with_suffix(".trace")), TMPDIR=str(root / "tmp"))
    process = subprocess.run([str(a.consumer.resolve())], env=env, text=True, capture_output=True, timeout=30)
    if process.returncode:
        raise RuntimeError((case["name"], process.stderr))
    observed = json.loads(process.stdout)
    (root / (path.stem + ".observed.json")).write_text(json.dumps(observed, ensure_ascii=False, indent=2) + "\n")
    if case["expected_calls"] is not None:
        assert observed["terminal"] == "requires_action", (case["name"], observed)
        assert observed["calls"] == case["expected_calls"], (case["name"], observed)
        assert observed["corrections"] == [], (case["name"], observed)
        assert observed["normalizations"] == int(case.get("normalization", True)), (case["name"], observed)
    else:
        expected_count = 2 if case.get("repeat_rejected") else 1
        assert observed["terminal"] == ("failed" if case.get("repeat_rejected") else "succeeded"), (case["name"], observed)
        assert observed["calls"] == [], (case["name"], observed)
        assert len(observed["corrections"]) == expected_count, (case["name"], observed)
        assert observed["normalizations"] == 0, (case["name"], observed)
        assert all(item["tools_executed"] is False and item["rejected"] == case["reply"] for item in observed["corrections"]), case["name"]
    results.append({"name": case["name"], "result": "PASS"})
report = {"seed": 20261020, "generated": 24, "cases": results, "boundary": "production public adapter with an external vendor wire fixture; no live inference or host tool execution"}
(root / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"cases": len(results), "generated": 24, "result": "PASS"}))
