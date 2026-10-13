"""Capture a genuine committed C0 boundary, then reopen only a disposable copied prefix.

The live source journal is never edited. SQLite's online backup supplies each coherent prefix;
the public owning evaluation API selects Running/Analyzing plus complete counts. At most three
actual campaigns and 256 snapshots are attempted. Only the selected snapshot metadata remains.
"""
import hashlib
import json
import os
import pathlib
import re
import shutil
import sqlite3
import subprocess
import sys
import time

seed = pathlib.Path(sys.argv[1]).resolve()
parent = pathlib.Path(sys.argv[2]).resolve()
binary = pathlib.Path(os.environ["PERITUS_RSI_CONSUMER"]).resolve()
digest = bytearray(hashlib.sha256(b"peritus.rsi.native-real:campaign-complete").digest()[:16])
digest[0] |= 1
campaign = digest.hex()
parent.mkdir(parents=True, exist_ok=True)
captured = None
snapshots = 0

for attempt, maximum in enumerate([85, 85, 86], start=1):
    root = parent / f"source-{attempt}"
    if root.exists():
        raise RuntimeError("refusing an existing source fixture")
    shutil.copytree(seed, root)
    snapshot_root = parent / f"snapshots-{attempt}"
    snapshot_root.mkdir()
    with (parent / f"source-{attempt}.log").open("wb") as output:
        owner = subprocess.Popen([str(binary), str(root), "complete", "start"], stdout=output, stderr=subprocess.STDOUT)
        started = time.monotonic()
        trigger = False
        while time.monotonic() - started < 180 and owner.poll() is None:
            try:
                state = json.loads((root / "native-complete-latest.json").read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                time.sleep(.005)
                continue
            counts = dict((key, int(value)) for key, value in re.findall(r"(expected|passed|task_failed|infrastructure_failed|cancelled|ambiguous): (\d+)", state["counts"]))
            settled = sum(value for key, value in counts.items() if key != "expected")
            if state["phase"] == "Running" and counts.get("expected") == 36 and settled >= 34:
                trigger = True
                break
            time.sleep(.005)
        if trigger:
            source = sqlite3.connect(f"file:{root / 'peritus.sqlite3'}?mode=ro", uri=True, timeout=2)
            source.execute("PRAGMA query_only=ON")
            for index in range(maximum):
                point = snapshot_root / f"point-{index:03d}"
                point.mkdir()
                with sqlite3.connect(point / "peritus.sqlite3") as target:
                    source.backup(target, pages=64, sleep=.002)
                snapshots += 1
                shutil.copyfile(root / "native-daemon.toml", point / "native-daemon.toml")
                time.sleep(.002)
            source.close()
        result = owner.wait(timeout=200)
        if result:
            raise RuntimeError(f"actual source campaign failed with {result}")
    candidates = []
    for point in sorted(snapshot_root.glob("point-*")):
        probe = subprocess.run([str(binary), str(point), "inspect-state", campaign], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
        if probe.returncode:
            continue
        state = json.loads(probe.stdout)
        if state["state"]["counts_complete"] and state["state"]["phase"] in ("Running", "Analyzing"):
            candidates.append((point, state))
    if candidates:
        point, state = next((entry for entry in candidates if entry[1]["state"]["phase"] == "Running"), candidates[0])
        selected_backup = parent / "selected-checkpoint.sqlite3"
        shutil.copyfile(point / "peritus.sqlite3", selected_backup)
        replay = parent / "replayed-prefix"
        subprocess.run(["cp", "-a", "--reflink=auto", str(root), str(replay)], check=True, timeout=60)
        # Replace only the disposable copy with an intact actual online backup; no row/history edits.
        shutil.copyfile(point / "peritus.sqlite3", replay / "peritus.sqlite3")
        for suffix in ("-wal", "-shm"):
            (replay / f"peritus.sqlite3{suffix}").unlink(missing_ok=True)
        before = len(list((replay / "rsi/rsi-child-receipts").glob("*.json")))
        with (parent / "prefix-resume.log").open("wb") as output:
            resumed = subprocess.run([str(binary), str(replay), "complete", "resume"], stdout=output, stderr=subprocess.STDOUT, timeout=200)
        after = len(list((replay / "rsi/rsi-child-receipts").glob("*.json")))
        if resumed.returncode or before != after:
            raise RuntimeError("actual committed-prefix restart failed or launched a new child")
        captured = {"attempt": attempt, "online_snapshot_state": state, "snapshot_sha256": hashlib.sha256(selected_backup.read_bytes()).hexdigest(), "selected_backup": str(selected_backup), "copied_replay": str(replay), "before_child_receipts": before, "after_child_receipts": after, "resume_exit": resumed.returncode, "result": json.loads((replay / "native-complete-result.json").read_text()), "shutdown": json.loads((replay / "native-complete-shutdown.json").read_text())}
        break

summary = {"snapshots": snapshots, "actual_campaign_attempts": attempt, "captured": captured, "coverage": "actual online C0 prefix restart" if captured else "bounded capture window not observed; no success claim"}
(parent / "actual-checkpoint-replay.json").write_text(json.dumps(summary, indent=2))
for directory in parent.glob("snapshots-*"):
    shutil.rmtree(directory)
print(json.dumps(summary, indent=2))
if captured is None:
    sys.exit(2)
