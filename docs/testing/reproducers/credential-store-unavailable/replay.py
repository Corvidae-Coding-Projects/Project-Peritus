#!/usr/bin/env python3
"""Replay provider setup with and without a reachable operating-system credential store.

Run this outside the repository sandbox: the campaign needs a session D-Bus, and the sandbox
denies socket creation and the OpenCode CLI version probe.

    python3 replay.py --binary target/debug/peritus --output /tmp/peritus-credential-store

Every scenario keeps its exact transcript. The last scenario asks the Secret Service itself for
the Peritus provider namespace, so a leftover credential cannot pass unnoticed.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import drive

PROMPT = b"Providers (comma-separated numbers"
KEY = b"sk-dummy-not-a-real-key\r"
KEY_PROMPT = b"Paste the API key and press Enter"
GUIDANCE = "operating-system credential store, which is not available here"
KEY_PROMPT_TEXT = "Paste the API key and press Enter"
SAVED = "Provider settings saved."


def base_env(root, name, extra):
    env = dict(os.environ)
    env.update(extra)
    env["XDG_CONFIG_HOME"] = f"{root}/{name}/config"
    env["XDG_STATE_HOME"] = f"{root}/{name}/state"
    env["XDG_CACHE_HOME"] = f"{root}/{name}/cache"
    env["TERM"] = "xterm-256color"
    return env


def without_store(env):
    env = dict(env)
    for name in (
        "DBUS_SESSION_BUS_ADDRESS",
        "DBUS_SESSION_BUS_PID",
        "GNOME_KEYRING_CONTROL",
        "SSH_AUTH_SOCK",
    ):
        env.pop(name, None)
    return env


def start_store():
    """Starts a session bus and an unlocked login keyring, as the operator guide instructs."""
    bus = subprocess.run(
        ["dbus-daemon", "--session", "--fork", "--print-address=1", "--print-pid=1"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    env = {"DBUS_SESSION_BUS_ADDRESS": bus[0], "DBUS_SESSION_BUS_PID": bus[1]}
    subprocess.run(
        ["gnome-keyring-daemon", "--daemonize", "--unlock", "--components=secrets"],
        input="\n",
        capture_output=True,
        text=True,
        env={**os.environ, **env},
    )
    time.sleep(1)
    return env


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/peritus")
    parser.add_argument("--output", default="/tmp/peritus-credential-store")
    parser.add_argument("--timeout", type=float, default=300)
    parser.add_argument(
        "--path-prefix",
        default="",
        help="directory holding a supported OpenCode 1.19.0, placed first on PATH",
    )
    options = parser.parse_args()
    binary = os.path.abspath(options.binary)
    root = os.path.abspath(options.output)
    if options.path_prefix:
        os.environ["PATH"] = os.path.abspath(options.path_prefix) + ":" + os.environ["PATH"]
    transcripts = os.path.join(root, "transcripts")
    os.makedirs(transcripts, exist_ok=True)

    results = []

    def scenario(name, env, sends, wipe=True):
        if wipe:
            shutil.rmtree(os.path.join(root, name), ignore_errors=True)
        out, code = drive([binary, "providers"], env, sends, timeout=options.timeout)
        text = out.decode("utf-8", "replace")
        with open(os.path.join(transcripts, f"{name}.txt"), "w", encoding="utf-8") as handle:
            handle.write(text + f"\n=== exit code: {code}\n")
        print(f"--- {name} (exit {code})\n{text}\n=== exit code: {code}\n")
        results.append((name, text, code))
        return text, code

    def check(name, expectation, observed):
        print(f"{'PASS' if observed else 'FAIL'} {name}: {expectation}")
        results.append((f"check {name}", expectation, observed))

    store_env = start_store()
    print(f"credential store: bus {store_env['DBUS_SESSION_BUS_ADDRESS']}")

    offline = without_store(base_env(root, "unavailable-openai-api", {}))
    text, code = scenario("unavailable-openai-api", offline, [(PROMPT, b"3\n")])
    check("unavailable-openai-api/guidance", "corrective guidance is printed", GUIDANCE in text)
    check("unavailable-openai-api/no-key-prompt", "no key is requested", KEY_PROMPT_TEXT not in text)
    check("unavailable-openai-api/exit", "setup exits 70", code == 70)

    offline = without_store(base_env(root, "unavailable-opencode-go", {}))
    text, code = scenario("unavailable-opencode-go", offline, [(PROMPT, b"8\n")])
    check("unavailable-opencode-go/guidance", "corrective guidance is printed", GUIDANCE in text)
    check("unavailable-opencode-go/no-key-prompt", "no key is requested", KEY_PROMPT_TEXT not in text)
    check("unavailable-opencode-go/exit", "setup exits 70", code == 70)

    work = base_env(root, "available-opencode-go", store_env)
    text, code = scenario(
        "available-opencode-go",
        work,
        [
            (PROMPT, b"8\n"),
            (KEY_PROMPT, KEY),
            (b"Choose a model number", b"manual local-opencode-go-placeholder\n"),
            (b"Test generation and tool calling now?", b"\n"),
            (b"Reviewer provider", b"\n"),
        ],
    )
    check("available-opencode-go/no-guidance", "guidance is absent", GUIDANCE not in text)
    check("available-opencode-go/key-prompt", "the key prompt appears", KEY_PROMPT_TEXT in text)
    check("available-opencode-go/stored", "the credential is stored", "Credential captured." in text)
    check("available-opencode-go/saved", "provider settings are saved", SAVED in text)
    check("available-opencode-go/exit", "setup exits 0", code == 0)

    text, code = scenario("cleanup-opencode-go", work, [(PROMPT, b"0\n")], wipe=False)
    check("cleanup-opencode-go/retained", "the stored route is retained", "selected, configured" in text)
    check(
        "cleanup-opencode-go/removal",
        "deselecting removes the credential without a warning",
        "could not be removed automatically" not in text,
    )
    check("cleanup-opencode-go/exit", "setup exits 0", code == 0)

    text, _ = scenario("after-cleanup-opencode-go", work, [(PROMPT, b"0\n")], wipe=False)
    check(
        "after-cleanup-opencode-go/dropped",
        "the route is gone",
        re.search(r"8\. OpenCode Go via OpenCode CLI +Add key", text) is not None,
    )

    inventory = os.path.join(os.path.dirname(os.path.abspath(__file__)), "secret-service-inventory.sh")
    listed = subprocess.run(["bash", inventory], capture_output=True, text=True).stdout
    print(listed)
    with open(os.path.join(transcripts, "secret-service-inventory.txt"), "w", encoding="utf-8") as handle:
        handle.write(listed)
    check(
        "secret-service-inventory/empty",
        "the provider namespace holds no credential",
        "aoao 0 0" in listed,
    )

    failures = [entry for entry in results if entry[2] is False]
    print(f"\n{len(results) - len(failures)} checks passed, {len(failures)} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
