#!/usr/bin/env python3
"""Register real folders through the TUI, initialize Git, and reopen fresh clients."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time


def operate(binary, project, environment, trust, implicit=False):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 0, 0))
    command = [str(binary)] if implicit else [str(binary), "open", str(project)]
    child = subprocess.Popen(command, stdin=slave, stdout=slave, stderr=slave,
                             cwd=project, env=environment, start_new_session=True)
    os.close(slave)
    raw = bytearray()
    trusted = selected = quit_sent = False
    connected_at = None
    deadline = time.monotonic() + 30
    try:
        while child.poll() is None and time.monotonic() < deadline:
            if not select.select([master], [], [], 0.1)[0]:
                continue
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            raw.extend(chunk)
            if b"\x1b[6n" in chunk:
                os.write(master, b"\x1b[1;1R")
            if not trusted and b"Trust this workspace? [Y/n]:" in raw:
                os.write(master, b"y\r" if trust else b"n\r")
                trusted = True
            if not selected and b"Providers (comma-separated numbers, or 0 for offline)" in raw:
                os.write(master, b"0\r")
                selected = True
            if connected_at is None and b"connected to daemon" in raw:
                connected_at = time.monotonic()
            if connected_at is not None and time.monotonic() - connected_at > 1 and not quit_sent:
                os.write(master, b"\x11")
                quit_sent = True
        child.wait(timeout=3)
        if child.returncode != 0 or connected_at is None:
            raise RuntimeError(f"real CLI failed: exit={child.returncode}; output={raw[-6000:]!r}")
        return bytes(raw)
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
        os.close(master)


def stop_fixture(binary, root, environment):
    state = root / "state" / "peritus" / "daemon"
    instance = state / "daemon.instance"
    if not instance.exists():
        return
    values = dict(line.split("=", 1) for line in instance.read_text().splitlines())
    endpoint = str(state / (values["endpoint"] + ".sock"))
    if len(endpoint.encode()) > 107:
        identity = hashlib.sha256(b"peritus/unix-runtime-endpoint/v1\0" + endpoint.encode()).hexdigest()[:32]
        endpoint = f"/tmp/peritus-{identity}/daemon.sock"
    result = subprocess.run([str(binary), "--endpoint", endpoint, "--timeout-seconds", "15",
                             "shutdown", "--wait"], env=environment, capture_output=True, timeout=20)
    if result.returncode != 0:
        raise RuntimeError(f"fixture shutdown failed: {result.stdout!r} {result.stderr!r}")


def profile(root):
    state = max((root / "state" / "peritus" / "product-state").glob("state-*.json"))
    state = json.loads(state.read_text())["workspaces"]
    return next(row for row in state["recent"] if row["workspace_id"] == state["active_workspace_id"])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--expect-mismatch", action="store_true")
    parser.add_argument("--case", choices=["plain", "unicode λ space", "current-directory", "nested-after-git"])
    parser.add_argument("--output", type=Path)
    options = parser.parse_args()
    binary = options.binary.resolve()
    results = []
    for name, implicit in [("plain", False), ("unicode λ space", False),
                           ("current-directory", True), ("nested-after-git", True)]:
        if options.case and name != options.case:
            continue
        with tempfile.TemporaryDirectory(prefix="pfgr-") as temporary:
            root = Path(temporary)
            project = root / name
            project.mkdir()
            content = b"keep this original file: \xce\xbb\n"
            (project / "notes.md").write_bytes(content)
            environment = os.environ.copy()
            for kind in ("config", "state", "cache"):
                environment[f"XDG_{kind.upper()}_HOME"] = str(root / kind)
            environment["TERM"] = "xterm-256color"
            try:
                operate(binary, project, environment, True)
                before = profile(root)
                subprocess.run(["git", "init", "--quiet", str(project)], check=True)
                reopen = project
                if name == "nested-after-git":
                    reopen = project / "crates" / "engine"
                    reopen.mkdir(parents=True)
                operate(binary, reopen, environment, False, implicit=implicit)
                after = profile(root)
                preserved = (before["workspace_id"] == after["workspace_id"]
                             and before["repository_identity"] == after["repository_identity"]
                             and after["direct_folder"] and after["trust"] == "trusted")
                expected = ((before["workspace_id"] != after["workspace_id"]
                             and not after["direct_folder"] and after["trust"] == "restricted")
                            if options.expect_mismatch else preserved)
                passed = expected and before["direct_folder"] and (project / "notes.md").read_bytes() == content
                row = {"case": name, "before_workspace": before["workspace_id"],
                       "after_workspace": after["workspace_id"], "identity_preserved": preserved,
                       "after_direct_folder": after["direct_folder"], "after_trust": after["trust"],
                       "file_preserved": (project / "notes.md").read_bytes() == content,
                       "status": "PASS" if passed else "FAIL"}
                results.append(row)
                print(json.dumps(row), flush=True)
                if not passed:
                    raise RuntimeError("folder identity contract failed")
            finally:
                stop_fixture(binary, root, environment)
    report = {"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "expected_legacy_mismatch": options.expect_mismatch, "results": results}
    if options.output:
        options.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"PASS: {len(results)} real folder registration/Git initialization/reopen sequences")


if __name__ == "__main__":
    main()
