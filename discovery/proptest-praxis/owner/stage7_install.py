#!/usr/bin/env python3
"""Generated, real-shell Linux package lifecycle checks in disposable user homes."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
LINUX = ROOT / "packaging/linux"
MANAGED = (
    ".local/bin/peritusd",
    ".local/bin/peritus",
    ".local/bin/peritus-tui",
    ".local/libexec/peritus/peritus-linux-sandbox-helper",
    ".local/share/peritus/peritus.service",
    ".local/share/peritus/Uninstall-Peritus.sh",
)
CANARIES = (
    ".local/bin/sibling-tool",
    ".local/share/peritus/user-notes.txt",
    ".config/peritus/settings.toml",
    ".local/state/peritus/conversations.db",
    ".local/share/peritus/logs/user.log",
    ".local/share/peritus/credentials/user.key",
)
PROFILE = "# user profile\nexport CUSTOM_USER_SETTING=kept\n"
PATH_LINE = 'export PATH="$HOME/.local/bin:$PATH"'


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write(path: Path, data: bytes, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(mode)


def fixture_bundle(path: Path, version: str) -> dict[str, bytes]:
    path.mkdir()
    script = (
        "#!/bin/sh\n"
        "case \"${1:-}\" in\n"
        f"  --version) printf '%s\\n' 'peritus {version}'; exit 0 ;;\n"
        "  automation) [ \"${2:-}\" = setup ] && exit 0 ;;\n"
        "esac\n"
        "exit 64\n"
    ).encode()
    content = {
        "bin/peritus": script,
        "bin/peritusd": f"daemon-{version}\n".encode(),
        "bin/peritus-tui": f"tui-{version}\n".encode(),
        "libexec/peritus-linux-sandbox-helper": f"helper-{version}\n".encode(),
        "share/peritus/peritus.service": f"unit-{version}\n".encode(),
        "Uninstall-Peritus.sh": (LINUX / "Uninstall-Peritus.sh").read_bytes(),
    }
    for relative, data in content.items():
        write(path / relative, data, 0o755 if relative.startswith(("bin/", "libexec/")) else 0o700)
    write(path / "Install-Peritus.sh", (LINUX / "Install-Peritus.sh").read_bytes(), 0o700)
    write(path / "Upgrade-Peritus.sh", (LINUX / "Upgrade-Peritus.sh").read_bytes(), 0o700)
    write(path / "manifest.toml", b"version = '0.0.0'\n")
    checks = []
    for file in sorted(path.rglob("*")):
        if file.is_file() and file.name != "SHA256SUMS":
            checks.append(f"{sha(file.read_bytes())}  {file.relative_to(path)}")
    write(path / "SHA256SUMS", ("\n".join(checks) + "\n").encode())
    return {
        MANAGED[0]: content["bin/peritusd"],
        MANAGED[1]: content["bin/peritus"],
        MANAGED[2]: content["bin/peritus-tui"],
        MANAGED[3]: content["libexec/peritus-linux-sandbox-helper"],
        MANAGED[4]: content["share/peritus/peritus.service"],
        MANAGED[5]: content["Uninstall-Peritus.sh"],
    }


def fixture_commands(directory: Path) -> None:
    for command in ("bwrap", "dbus-daemon", "gnome-keyring-daemon"):
        write(directory / command, b"#!/bin/sh\nexit 0\n", 0o755)
    write(
        directory / "install",
        b"#!/bin/sh\n"
        b"if [ \"${1:-}\" = -m ]; then\n"
        b"  count=$(cat \"$PRACTICE_INSTALL_COUNT\")\n"
        b"  count=$((count + 1))\n"
        b"  printf '%s\\n' \"$count\" > \"$PRACTICE_INSTALL_COUNT\"\n"
        b"  if [ \"$count\" -eq \"${PRACTICE_FAIL_AT:-0}\" ]; then\n"
        b"    printf '%s\\n' 'injected file publication failure' >&2\n"
        b"    exit 74\n"
        b"  fi\n"
        b"fi\n"
        b"exec /usr/bin/install \"$@\"\n",
        0o755,
    )


def contents(home: Path) -> dict[str, tuple[str, int]]:
    result = {}
    for path in home.rglob("*"):
        if path.is_file():
            result[str(path.relative_to(home))] = (sha(path.read_bytes()), path.stat().st_mode & 0o777)
    return result


def invoke(script: Path, bundle: Path | None, env: dict[str, str]) -> dict[str, object]:
    args = ["/bin/sh", str(script)]
    if bundle is not None:
        args.append(str(bundle))
    process = subprocess.run(args, env=env, capture_output=True, text=True, timeout=30, check=False)
    return {"command": args, "exit_code": process.returncode,
            "stdout": process.stdout, "stderr": process.stderr}


def case_run(case: dict[str, object], retain: Path | None = None) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="peritus-stage7-install-") as temp:
        root = Path(temp)
        home = root / "home"
        home.mkdir()
        tempdir = root / "tmp"
        tempdir.mkdir()
        stubs = root / "stubs"
        stubs.mkdir()
        fixture_commands(stubs)
        count = root / "install-count"
        count.write_text("0\n")
        bundle = root / "bundle"
        bundle_files = fixture_bundle(bundle, "7.0.0")
        for index, relative in enumerate(CANARIES):
            write(home / relative, f"user-canary-{index}\n".encode())
        write(home / ".bashrc", PROFILE.encode())
        initial = str(case["initial"])
        if initial == "full":
            for relative in MANAGED:
                write(home / relative, ("old-" + relative + "\n").encode(), 0o755)
        env = dict(os.environ)
        env.update(HOME=str(home), TMPDIR=str(tempdir), SHELL="/bin/bash",
                   PATH=f"{stubs}:/usr/bin:/bin", PERITUS_INSTALL_DEPS="0",
                   PRACTICE_INSTALL_COUNT=str(count), PRACTICE_FAIL_AT=str(case["fail_at"]))
        setup = None
        if initial == "partial":
            env["PRACTICE_FAIL_AT"] = "3"
            setup = invoke(bundle / "Install-Peritus.sh", bundle, env)
            if setup["exit_code"] == 0 or not (home / MANAGED[1]).is_file():
                raise RuntimeError("failed initial install did not reach the bootstrap upgrade route")
            count.write_text("0\n")
            env["PRACTICE_FAIL_AT"] = str(case["fail_at"])
        before = contents(home)
        action = str(case["action"])
        script = bundle / ("Upgrade-Peritus.sh" if action == "upgrade" else "Install-Peritus.sh")
        first = invoke(script, bundle, env)
        after = contents(home)
        problems = []
        fail_at = int(case["fail_at"])
        expected_fail = fail_at > 0
        if (first["exit_code"] != 0) != expected_fail:
            problems.append(f"exit outcome differs from injected publication failure {fail_at}")
        for relative in CANARIES:
            if after.get(relative) != before.get(relative):
                problems.append(f"user or sibling canary changed: {relative}")
        if expected_fail:
            if "Peritus installed" in str(first["stdout"]):
                problems.append("reported installation success after publication failure")
            if action == "upgrade":
                for relative in MANAGED:
                    if after.get(relative) != before.get(relative):
                        problems.append(f"failed upgrade did not restore {relative}")
        else:
            for relative, data in bundle_files.items():
                if after.get(relative, (None,))[0] != sha(data):
                    problems.append(f"successful {action} did not publish {relative}")
            profile = (home / ".bashrc").read_text()
            if profile.count(PATH_LINE) != 1 or not profile.startswith(PROFILE):
                problems.append("successful installation did not preserve and extend profile once")
        second = None
        if bool(case.get("retry")):
            count.write_text("0\n")
            env["PRACTICE_FAIL_AT"] = "0"
            second = invoke(script, bundle, env)
            retried = contents(home)
            if second["exit_code"] != 0:
                problems.append("retry failed after publication fault")
            for relative in CANARIES:
                if retried.get(relative) != before.get(relative):
                    problems.append(f"retry changed user canary: {relative}")
            for relative, data in bundle_files.items():
                if retried.get(relative, (None,))[0] != sha(data):
                    problems.append(f"retry did not publish {relative}")
            profile = (home / ".bashrc").read_text()
            if profile.count(PATH_LINE) != 1 or not profile.startswith(PROFILE):
                problems.append("retry changed profile or duplicated its path entry")
        removal = None
        if bool(case.get("uninstall")) and (not expected_fail or bool(case.get("retry"))):
            removal = invoke(bundle / "Uninstall-Peritus.sh", None, env)
            retired = contents(home)
            if removal["exit_code"] != 0:
                problems.append("uninstall failed after a completed installation")
            for relative in MANAGED:
                if relative in retired:
                    problems.append(f"uninstall retained package path: {relative}")
            for relative in CANARIES:
                if retired.get(relative) != before.get(relative):
                    problems.append(f"uninstall changed user canary: {relative}")
            if (home / ".bashrc").read_text().count(PATH_LINE) != 1:
                problems.append("uninstall changed the user's shell profile")
        if retain is not None:
            retain.mkdir(parents=True)
            (retain / "case.json").write_text(json.dumps(case, indent=2, sort_keys=True) + "\n")
            (retain / "observation.json").write_text(json.dumps({"setup": setup, "first": first, "second": second,
                "removal": removal,
                "before": before, "after": after, "problems": problems}, indent=2, sort_keys=True) + "\n")
        return {"status": "pass" if not problems else "fail", "case": case,
                "first": first, "second": second, "removal": removal, "problems": problems}


def generated(seed: int, count: int) -> list[dict[str, object]]:
    rng = random.Random(seed)
    cases = [
        {"initial": "clean", "action": "install", "fail_at": 0, "retry": True, "uninstall": True},
        {"initial": "full", "action": "upgrade", "fail_at": 3, "retry": True},
        {"initial": "partial", "action": "upgrade", "fail_at": 4, "retry": True},
    ]
    for _ in range(count):
        initial = rng.choice(("clean", "full", "partial"))
        action = "install" if initial == "clean" else "upgrade"
        fail_at = rng.randrange(7)
        retry = rng.choice((True, False))
        cases.append({"initial": initial, "action": action,
                      "fail_at": fail_at, "retry": retry,
                      "uninstall": (fail_at == 0 or retry) and rng.choice((True, False))})
    return cases


def main() -> int:
    global ROOT, LINUX
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=7319)
    parser.add_argument("--cases", type=int, default=96)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--replay", type=Path)
    parser.add_argument("--source-root", type=Path, help="candidate checkout containing packaging/linux")
    args = parser.parse_args()
    if args.source_root is not None:
        ROOT = args.source_root.resolve()
        LINUX = ROOT / "packaging/linux"
    if args.output.exists():
        parser.error("output must not exist")
    args.output.mkdir(parents=True, mode=0o700)
    cases = [json.loads(args.replay.read_text())] if args.replay else generated(args.seed, args.cases)
    first_failure = None
    metrics = {"cases": 0, "successful_installs": 0, "injected_failures": 0,
               "retries": 0, "partial_prior_state": 0, "uninstalls": 0}
    for index, case in enumerate(cases):
        result = case_run(case, args.output / f"case-{index:04d}")
        metrics["cases"] += 1
        metrics["successful_installs"] += int(case["fail_at"] == 0)
        metrics["injected_failures"] += int(case["fail_at"] != 0)
        metrics["retries"] += int(bool(case["retry"]))
        metrics["partial_prior_state"] += int(case["initial"] == "partial")
        metrics["uninstalls"] += int(bool(case.get("uninstall")))
        if result["status"] != "pass":
            first_failure = result
            (args.output / "first-failure.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
            break
    source = {str(path.relative_to(ROOT)): sha(path.read_bytes()) for path in
              (LINUX / "Install-Peritus.sh", LINUX / "Upgrade-Peritus.sh",
               LINUX / "Uninstall-Peritus.sh")}
    source["discovery_harness"] = sha(Path(__file__).read_bytes())
    report = {"status": "pass" if first_failure is None else "fail",
              "seed": args.seed, "requested_generated_cases": args.cases,
              "completed_cases": metrics["cases"], "metrics": metrics,
              "source_sha256": source, "first_failure": first_failure,
              "interface": "actual Linux /bin/sh package lifecycle scripts in disposable HOME"}
    (args.output / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in ("status", "seed", "completed_cases", "metrics")}))
    return 0 if first_failure is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
