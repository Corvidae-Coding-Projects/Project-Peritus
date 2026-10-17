#!/usr/bin/env python3
"""Operate the real provider-settings terminal with disposable Peritus state."""

import argparse
import json
import os
from pathlib import Path

import pexpect

parser = argparse.ArgumentParser()
parser.add_argument("--binary", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
root = args.output.resolve()
root.mkdir(parents=True, exist_ok=False)
env = os.environ.copy()
env.update({f"XDG_{kind}_HOME": str(root / folder) for kind, folder in (
    ("CONFIG", "config"), ("STATE", "state"), ("CACHE", "cache"),
)})


def state():
    files = (root / "state/peritus/product-state").glob("state-*.json")
    return json.loads(max(files).read_text())


def settings(label, actions, cancelled=False):
    child = pexpect.spawn(str(binary), ["providers"], env=env, cwd=str(root),
                          encoding="utf-8", timeout=80, dimensions=(40, 160))
    with (root / f"{label}.txt").open("w") as log:
        child.logfile_read = log
        try:
            for prompt, answer in actions:
                child.expect(prompt)
                child.sendline(answer)
            child.expect(pexpect.EOF)
            child.close()
            assert child.signalstatus is None
            assert (child.exitstatus != 0) if cancelled else (child.exitstatus == 0)
        finally:
            if child.isalive():
                child.terminate(force=True)
    print(f"PASS: {label}")
    return state()


selected = settings("choose-reviewer", [
    ("Providers .*: ", "1"),
    ("Choose a model number.*: ", "manual gpt-6-luna"),
    ("Reviewer provider .*: ", "999"),
    ("Reviewer provider .*: ", "1"),
    ("Choose a model number.*: ", "manual bad model"),
    ("Choose a model number.*: ", "manual gpt-6.1-sol"),
])["providers"]
assert selected["account_models"] == {"codex-account": "gpt-6-luna"}
assert selected["reviewer"] == {
    "provider": "codex-account", "model": "gpt-6.1-sol", "manual": True,
}
retained = settings("retain-reviewer", [
    ("Providers .*: ", ""), ("Reviewer provider .*: ", ""),
    ("Choose a model number.*: ", ""),
])["providers"]
assert retained["reviewer"] == selected["reviewer"]
assert retained["account_models"] == selected["account_models"]
default = settings("follow-default", [
    ("Providers .*: ", ""), ("Reviewer provider .*: ", "0"),
])
assert "reviewer" not in default["providers"]
cancelled = settings("cancel-reviewer", [
    ("Providers .*: ", ""), ("Reviewer provider .*: ", "1"),
    ("Choose a model number.*: ", "q"),
], cancelled=True)
assert cancelled == default
