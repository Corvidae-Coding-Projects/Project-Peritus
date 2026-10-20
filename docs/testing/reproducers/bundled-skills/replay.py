#!/usr/bin/env python3
"""Build and operate a native skill consumer. No unit-test targets or provider requests."""

import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[4]
DATA = Path(os.environ.get("PERITUS_SKILL_REPLAY_DIR", Path.home() / ".cache/peritus-skills-20261020"))
DATA.mkdir(parents=True, exist_ok=True)
CONSUMER = DATA / "consumer"
CONSUMER.mkdir(exist_ok=True)
RNG = random.Random(20261020)
REQUESTS = 0
PAGES = 0


def build(label):
    with (DATA / f"{label}-build.log").open("w") as log:
        subprocess.run([
            "nice", "-n", "10", "cargo", "build", "--offline", "--manifest-path",
            str(CONSUMER / "Cargo.toml"), "--target-dir", str(ROOT / "target"), "-j", "2",
        ], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
    return ROOT / "target/debug/skill-bundle-consumer"


def ask(binary, request):
    global REQUESTS
    result = subprocess.run([str(binary)], input=json.dumps(request) + "\n", text=True,
                            capture_output=True, check=True)
    response = json.loads(result.stdout)
    with (DATA / "observations.jsonl").open("a") as log:
        log.write(json.dumps({"request": request, "response": response}, ensure_ascii=False) + "\n")
    REQUESTS += 1
    return response


def pages(binary, op, skill=None, path=None, maximum=1024):
    global PAGES
    request = {"op": op, "max_bytes": maximum, "offset": 0}
    if skill is not None:
        request["skill"] = skill
    if path is not None:
        request["path"] = path
    data = bytearray()
    digest = None
    while True:
        response = ask(binary, request)
        assert "error" not in response, response
        assert len(json.dumps(response, ensure_ascii=False, separators=(",", ":")).encode()) <= maximum
        assert response["offset"] == len(data)
        chunk = bytes.fromhex(response["bytes"]) if response.get("encoding") == "hex" else response["bytes"].encode()
        data.extend(chunk)
        digest = digest or response["sha256"]
        assert response["sha256"] == digest
        PAGES += 1
        if response["next_offset"] is None:
            assert len(data) == response["total_bytes"]
            assert hashlib.sha256(data).hexdigest() == digest
            return bytes(data)
        assert chunk and response["next_offset"] == len(data)
        request["offset"] = response["next_offset"]


def snapshot():
    return {directory.name: {file.relative_to(directory).as_posix(): file.read_bytes()
                            for file in directory.rglob("*") if file.is_file()}
            for directory in (ROOT / "skills").iterdir()
            if directory.is_dir() and (directory / "SKILL.md").is_file()}


def check(binary, expected, label):
    inventory = ask(binary, {"op": "inventory"})
    names = [item["skill"] for item in inventory]
    assert names == sorted(expected), names
    listed = json.loads(pages(binary, "list", maximum=384))
    assert [item["skill"] for item in listed] == names
    for item in inventory:
        skill = item["skill"]
        resources = item["resources"]
        assert [entry["path"] for entry in resources] == sorted(expected[skill])
        metadata = json.loads(pages(binary, "list", skill, maximum=512))
        assert [entry["path"] for entry in metadata] == sorted(expected[skill])
        for entry in resources:
            path = entry["path"]
            actual = pages(binary, "read", skill, path, RNG.choice([256, 384, 1024, 16384]))
            assert actual == expected[skill][path], (skill, path)
            assert entry["total_bytes"] == len(actual)
    print(f"PASS {label}: {len(names)} skills; {sum(map(len, expected.values()))} resources", flush=True)


def main():
    dependency = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["dependencies"]["serde_json"]
    version = dependency if isinstance(dependency, str) else dependency["version"]
    (CONSUMER / "Cargo.toml").write_text(
        '[package]\nname="skill-bundle-consumer"\nversion="0.0.0"\nedition="2024"\n[workspace]\n'
        '[dependencies]\nperitus-product-runner={path=' + json.dumps(str(ROOT / "crates/app/peritus-product-runner")) + '}\n'
        'serde_json=' + json.dumps(version) + '\n[[bin]]\nname="skill-bundle-consumer"\npath="main.rs"\ntest=false\nbench=false\n')
    shutil.copyfile(Path(__file__).with_name("consumer.rs"), CONSUMER / "main.rs")
    if not (CONSUMER / "Cargo.lock").exists():
        shutil.copyfile(ROOT / "Cargo.lock", CONSUMER / "Cargo.lock")
    (DATA / "observations.jsonl").write_text("")
    first = ROOT / "skills/replay-canonical-skill"
    second = ROOT / "skills/replay-plain-π"
    if first.exists() or second.exists() or (DATA / "hidden-source").exists():
        raise RuntimeError("owned fixture paths already exist; refusing to replace them")
    baseline = snapshot()
    binary = build("baseline")
    check(binary, baseline, "canonical packages")
    owned = []
    try:
        for directory in [first, second]:
            directory.mkdir()
            owned.append(directory)
        (first / "SKILL.md").write_text('---\r\nname: ignored-display-name\r\ndescription: >-\r\n  A drop-in skill with "quotes"\r\n  and Unicode π.\r\n---\r\n\r\n# Discovery fixture\r\n')
        (second / "SKILL.md").write_text("# Plain heading fallback\n\nNo front matter.\n")
        for path, data in {
            "references/a.md": ('π🙂 \\"\n' * 400).encode(),
            "references/a/nested.txt": b"nested resource\r\n",
            "data/raw.bin": bytes([255, 0, 128, 254]) + RNG.randbytes(4097),
            "empty": b"",
            'space dir/quoted"ñ.txt': "π starts here\n".encode(),
            "scripts/never-run.sh": b"#!/bin/sh\nprintf forbidden > SHOULD_NOT_EXIST\n",
        }.items():
            target = first / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        expected = snapshot()
        binary = build("added")
        check(binary, expected, "drop-in packages without Rust registration")
        listed = ask(binary, {"op": "inventory"})
        descriptions = {item["skill"]: item["description"] for item in listed}
        assert descriptions[first.name] == 'A drop-in skill with "quotes" and Unicode π.'
        assert descriptions[second.name] == "Plain heading fallback"
        for request in [
            {"op": "read", "skill": first.name, "path": "../real-testing/SKILL.md"},
            {"op": "read", "skill": first.name, "path": str(first / "SKILL.md")},
            {"op": "read", "skill": "missing-skill", "path": "SKILL.md"},
            {"op": "list", "skill": "missing-skill"},
            {"op": "read", "skill": first.name, "path": 'space dir/quoted"ñ.txt', "offset": 1},
            {"op": "read", "skill": first.name, "path": "empty", "offset": 1},
            *[{"op": "list", "max_bytes": size} for size in [0, 255, 524289]],
        ]:
            assert "error" in ask(binary, request), request
        for path in ["empty", "data/raw.bin", 'space dir/quoted"ñ.txt']:
            result = ask(binary, {"op": "read", "skill": first.name, "path": path,
                                  "offset": len(expected[first.name][path])})
            assert result["bytes"] == "" and result["next_offset"] is None
        print("PASS invalid names, traversal, absolute paths, offsets, page budgets and EOF", flush=True)
        (first / "empty").unlink()
        (first / "references/a.md").write_text("updated source π\n")
        (first / "data/new.unknown-extension").write_bytes(b"\xffnew arbitrary bytes\x00")
        expected = snapshot()
        binary = build("changed")
        check(binary, expected, "automatic updates, nested additions and deletions")
        first.rename(DATA / "hidden-source")
        owned.remove(first)
        try:
            check(binary, expected, "source-independent compiled resources")
        finally:
            (DATA / "hidden-source").rename(first)
            owned.append(first)
        assert not (ROOT / "SHOULD_NOT_EXIST").exists()
    finally:
        for directory in owned:
            shutil.rmtree(directory)
    binary = build("removed")
    check(binary, baseline, "removed packages disappear on rebuild")
    summary = {"seed": 20261020, "requests": REQUESTS, "pages": PAGES, "result": "PASS"}
    (DATA / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
