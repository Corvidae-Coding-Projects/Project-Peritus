#!/usr/bin/env python3
"""Observe the real vendor CLI without altering its input, output, status, or environment."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading

payload = sys.stdin.buffer.read()
child = subprocess.Popen(
    [os.environ["PERITUS_REAL_VENDOR_BINARY"], *sys.argv[1:]],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
)
prefix = Path(os.environ["PERITUS_REAL_CAPTURE_DIR"]) / str(os.getpid()) if "run" in sys.argv[1:] else None


def relay(source, destination, suffix):
    capture = None
    if prefix is not None:
        path = prefix.with_suffix(suffix)
        capture = path.open("wb")
        path.chmod(0o600)
    try:
        while data := source.read1(4096):
            if capture is not None:
                capture.write(data)
                capture.flush()
            destination.write(data)
            destination.flush()
    finally:
        if capture is not None:
            capture.close()


threads = [threading.Thread(target=relay, args=args) for args in (
    (child.stdout, sys.stdout.buffer, ".stdout"),
    (child.stderr, sys.stderr.buffer, ".stderr"),
)]
for thread in threads:
    thread.start()
try:
    child.stdin.write(payload)
except BrokenPipeError:
    pass
finally:
    child.stdin.close()
code = child.wait()
for thread in threads:
    thread.join()
if prefix is not None:
    prefix.with_suffix(".meta.json").write_text(json.dumps({
        "model": sys.argv[sys.argv.index("--model") + 1],
        "correction": b"PERITUS TRANSPORT CORRECTION" in payload,
        "input_sha256": hashlib.sha256(payload).hexdigest(),
    }) + "\n")
raise SystemExit(code)
