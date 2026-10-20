#!/usr/bin/env python3
"""Operate the real trace CLI with isolated files, captured provider events, and RSS measurements.

Expectations: unchanged human/JSON contents and record indices, fail-closed corrupt records,
ignored torn tails, constant memory with fixed record size, and header-only skipping of a sparse
10 GiB irrelevant record. No daemon, provider inference, or user state is opened.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--cli", type=Path, required=True)
p.add_argument("--baseline", type=Path, required=True)
p.add_argument("--provider-corpus", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
root = a.output.resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)
for name in ["state", "config", "cache", "tmp"]:
    (root / name).mkdir(mode=0o700)
directory = root / "state/peritus/daemon/product-runs"
directory.mkdir(parents=True)
identity = "d515d515d515d515d515d515d515d515"
trace = directory / (identity + ".trace")
env = dict(os.environ, XDG_STATE_HOME=str(root / "state"), XDG_CONFIG_HOME=str(root / "config"),
           XDG_CACHE_HOME=str(root / "cache"), TMPDIR=str(root / "tmp"))

def frame(kind, payload):
    return struct.pack("<BQ", kind, len(payload)) + payload

def command(binary, json_output=False, extensions=False):
    result = [str(binary.resolve())]
    if json_output: result.append("--json")
    result.extend(["runs", "trace", "--run", identity])
    if extensions: result.append("--extensions")
    return result

def invoke(binary, json_output=False, extensions=False, environment=env):
    return subprocess.run(command(binary, json_output, extensions), env=environment, capture_output=True, timeout=30)

checks = []
captured = a.provider_corpus / "000-literal-0-calls-0.trace"
assert captured.is_file(), captured
mixed = frame(2, b'{"call_id":"prefix","output":"before"}') + captured.read_bytes() + frame(4, b'{"retry":2}')
trace.write_bytes(mixed)
for json_output in [False, True]:
    for extensions in [False, True]:
        before = invoke(a.baseline, json_output, extensions)
        after = invoke(a.cli, json_output, extensions)
        assert before.returncode == after.returncode == 0, (before.stderr, after.stderr)
        if json_output:
            expected, observed = json.loads(before.stdout), json.loads(after.stdout)
            assert observed == expected, (expected, observed)
            assert observed["result"], "captured normalization must be visible"
            if extensions:
                assert all(row["event"] == "extension" for row in observed["result"])
                assert observed["result"][0]["index"] > 1
        else:
            assert before.stdout == after.stdout, (before.stdout, after.stdout)
        checks.append({"case": "captured-provider-output", "json": json_output, "extensions": extensions, "result": "PASS"})

ordinary = frame(2, b'{"text":"retained"}')
for suffix in [b"", *[bytes([2]) + b"\0" * n for n in range(8)], struct.pack("<BQ", 2, 100) + b"short"]:
    trace.write_bytes(ordinary + suffix)
    observed = invoke(a.cli, True)
    assert observed.returncode == 0, observed.stderr
    assert json.loads(observed.stdout)["result"] == [{"index": 1, "kind": "tool-observation", "payload": {"text": "retained"}}]
checks.append({"case": "complete-record-and-torn-tail", "variants": 10, "result": "PASS"})

for corrupt in [frame(255, b"unknown"), frame(1, b"not-a-provider-envelope")]:
    for extensions in [False, True]:
        trace.write_bytes(ordinary + corrupt)
        observed = invoke(a.cli, True, extensions)
        assert observed.returncode == 14, (observed.returncode, observed.stderr)
        assert observed.stdout == b"" and json.loads(observed.stderr)["ok"] is False, (observed.stdout, observed.stderr)
        assert not list((root / "tmp").iterdir()), "temporary output must be cleaned on failure"
checks.append({"case": "corruption-no-partial-success", "variants": 4, "result": "PASS"})

trace.write_bytes(ordinary)
bad_tmp = dict(env, TMPDIR=str(root / "missing-temporary-directory"))
observed = invoke(a.cli, True, environment=bad_tmp)
assert observed.returncode == 13 and observed.stdout == b"" and json.loads(observed.stderr)["ok"] is False, observed
checks.append({"case": "temporary-storage-failure", "result": "PASS"})

payload = json.dumps({"call_id": "read", "name": "workspace_read", "arguments": '{"path":"design.md"}',
                      "output": json.dumps({"text": "x" * (64 * 1024)}), "is_error": False}, separators=(",", ":")).encode()
record = frame(2, payload)
measurements = []
for label, binary in [("baseline", a.baseline), ("fixed", a.cli)]:
    for count in [64, 256, 1024]:
        with trace.open("wb") as stream:
            for _ in range(count): stream.write(record)
        for json_output in [False, True]:
            for extensions in [False, True]:
                rss = root / f"{label}-{count}-{json_output}-{extensions}.rss"
                with (root / (rss.stem + ".stderr")).open("wb") as error:
                    result = subprocess.run(["/usr/bin/time", "-f", "%M", "-o", str(rss), *command(binary, json_output, extensions)],
                                            env=env, stdout=subprocess.DEVNULL, stderr=error, timeout=60)
                assert result.returncode == 0, rss
                row = {"binary": label, "records": count, "trace_bytes": trace.stat().st_size, "json": json_output,
                       "extensions": extensions, "maximum_rss_kib": int(rss.read_text().strip())}
                measurements.append(row)
                print(json.dumps(row), flush=True)
for json_output in [False, True]:
    for extensions in [False, True]:
        series = [r["maximum_rss_kib"] for r in measurements if r["binary"] == "fixed" and r["json"] == json_output and r["extensions"] == extensions]
        assert max(series) - min(series) <= 8192, series
checks.append({"case": "memory-independent-of-total-trace-size", "runs": len(measurements), "allowed_rss_variation_kib": 8192, "result": "PASS"})

size = 10 * 1024**3
with trace.open("wb") as stream:
    stream.write(struct.pack("<BQ", 2, size))
    stream.seek(9 + size - 1)
    stream.write(b"X")
observed = invoke(a.cli, True, True)
assert observed.returncode == 0 and json.loads(observed.stdout)["result"] == [], observed.stderr
checks.append({"case": "skip-sparse-10-GiB-payload", "logical_bytes": trace.stat().st_size, "result": "PASS"})
trace.unlink()

def digest(path):
    with path.open("rb") as stream: return hashlib.file_digest(stream, "sha256").hexdigest()

report = {"cli": str(a.cli.resolve()), "cli_sha256": digest(a.cli), "baseline_sha256": digest(a.baseline),
          "checks": checks, "measurements": measurements, "boundary": "actual Linux CLI, disposable local trace files; no leak or native cross-platform claim"}
(root / "results.json").write_text(json.dumps(report, indent=2) + "\n")
assert not list((root / "tmp").iterdir())
print(json.dumps({"functional_groups": len(checks), "memory_runs": len(measurements), "result": "PASS"}))
