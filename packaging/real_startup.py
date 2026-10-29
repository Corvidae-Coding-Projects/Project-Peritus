"""Install and operate an actual Unix release package on a disposable CI runner.

Uses the runner's existing home without redefining HOME. Refuses existing Peritus
state or installations. No provider inference, unit tests, or simulated daemons.
"""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import pty
import select
import struct
import subprocess
import termios
import time
import tomllib


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("startup replay requires a disposable GitHub Actions runner")
    system = platform.system()
    if system not in ("Linux", "Darwin"):
        raise RuntimeError("Unix startup replay requires Linux or macOS")
    repo = Path(__file__).resolve().parent.parent
    record = json.loads((repo / "dist/peritus-native-package.json").read_text())
    archive = repo / record["archive"]
    user_home = Path(os.environ["HOME"])
    application = user_home / "Library/Application Support/Peritus"
    state = (application / "State" if system == "Darwin" else
             Path(os.environ.get("XDG_STATE_HOME", user_home / ".local/state")) / "peritus")
    config = (application / "Config" if system == "Darwin" else
              Path(os.environ.get("XDG_CONFIG_HOME", user_home / ".config")) / "peritus")
    command = user_home / ".local/bin/peritus"
    for existing in (state, config, command, application):
        if existing.exists() or existing.is_symlink():
            raise RuntimeError(f"refusing a nonempty Peritus runner environment: {existing}")
    evidence = repo / "dist/startup-evidence"
    evidence.mkdir()
    workspace = evidence / "workspace"
    workspace.mkdir()
    # Exercise normal dependency setup on both platforms: Macs also need the
    # private Python/Playwright/Chromium runtime, even when Git is preinstalled.
    version = tomllib.loads((repo / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    mirror = repo / "dist/startup-mirror" / ("v" + version)
    mirror.mkdir(parents=True)
    for source in (archive, archive.with_name(archive.name + ".sha256")):
        os.link(source, mirror / source.name)
    broken_wrapper = evidence / "non-executable-sccache"
    broken_wrapper.write_text("#!/bin/sh\nexec \"$@\"\n")
    broken_wrapper.chmod(0o600)
    environment = dict(os.environ, PERITUS_INSTALL_DEPS="1", PERITUS_VERSION="v" + version,
                       PERITUS_RELEASE_BASE_URL=(repo / "dist/startup-mirror").as_uri(),
                       RUSTC_WRAPPER=str(broken_wrapper), RUSTC_WORKSPACE_WRAPPER=str(broken_wrapper))
    installed = subprocess.run(["sh", str(repo / "install.sh")],
                               env=environment, capture_output=True, text=True, timeout=300)
    (evidence / "install.log").write_text(installed.stdout + installed.stderr)
    if installed.returncode:
        raise RuntimeError("actual release installer failed; see install.log")
    if system == "Darwin":
        if not command.is_symlink() or (command.parent / "peritusd").exists():
            raise RuntimeError("Mac command path did not reproduce the separate CLI alias layout")
        if command.resolve() != application / "bin/peritus":
            raise RuntimeError("Mac alias resolved outside the installed package")

    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 120, 0, 0))
    client = subprocess.Popen([str(command), "open", str(workspace)], stdin=slave,
                              stdout=slave, stderr=slave, start_new_session=True)
    raw = bytearray()
    trust_answered = provider_answered = connected = False
    client_exit = None
    try:
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if not select.select([master], [], [], 0.1)[0]:
                if client.poll() is not None:
                    break
                continue
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            raw.extend(data)
            if b"Trust this workspace?" in raw and not trust_answered:
                os.write(master, b"n\n")
                trust_answered = True
            if b"Providers (comma-separated numbers" in raw and not provider_answered:
                os.write(master, b"0\n")
                provider_answered = True
            if b"\x1b[?u" in data:
                os.write(master, b"\x1b[?0u\x1b[?1;2c")
            if b"\x1b[6n" in data:
                os.write(master, b"\x1b[1;1R")
            if b"connected to daemon" in raw:
                connected = True
                break
        os.write(master, b"\x11")  # Actual Ctrl-Q, also on a failed/unfinished client.
        try:
            client_exit = client.wait(timeout=15)
        except subprocess.TimeoutExpired:
            client.terminate()
            client_exit = client.wait(timeout=10)
    finally:
        (evidence / "terminal.ansi").write_bytes(raw)
        os.close(master)
        os.close(slave)
        instance = state / "daemon/daemon.instance"
        if instance.exists():
            fields = dict(line.split("=", 1) for line in instance.read_text().splitlines())
            endpoint = str(instance.parent / (fields["endpoint"] + ".sock"))
            # Unix socket paths use the same documented launcher shortening on long roots.
            if len(endpoint.encode()) > (107 if system == "Linux" else 103):
                digest = hashlib.sha256(b"peritus/unix-runtime-endpoint/v1\0" + endpoint.encode()).hexdigest()[:32]
                endpoint = "/tmp/peritus-" + digest + "/daemon.sock"
            shutdown = subprocess.run([str(command), "--endpoint", endpoint, "shutdown", "--wait"],
                                      capture_output=True, text=True, timeout=45)
            (evidence / "shutdown.log").write_text(shutdown.stdout + shutdown.stderr)
            clean = shutdown.returncode == 0 and "shutdown clean; remaining=0" in shutdown.stdout
        else:
            clean = False
    result = {"platform": system, "command_is_symlink": command.is_symlink(),
              "offline_setup": provider_answered, "connected": connected,
              "client_exit": client_exit, "clean_shutdown": clean, "bootstrap_install_with_broken_wrappers": installed.returncode == 0}
    (evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, sort_keys=True))
    if not (provider_answered and connected and client_exit == 0 and clean):
        raise RuntimeError("actual installed startup journey failed; see startup-evidence")


if __name__ == "__main__":
    main()
