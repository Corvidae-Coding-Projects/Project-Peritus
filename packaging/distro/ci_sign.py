"""Explicit CI-only import into an ephemeral keyring; local signing never exports a key."""

import os
from pathlib import Path
import re
import subprocess
import tempfile

from common import run, version
from sign import fingerprint, sign


def seed_restricted_cache(key, passphrase):
    """The extra socket has a separate cache; a normal loopback signature does not seed it."""
    metadata = run("gpg", "--batch", "--with-colons", "--with-keygrip",
                   "--list-secret-keys", key, capture=True)
    grips = {line.split(":")[9] for line in metadata.splitlines() if line.startswith("grp:")}
    if not grips or any(not re.fullmatch(r"[A-F0-9]{40}", grip) for grip in grips):
        raise ValueError("CI signing key has no valid agent keygrips")
    preset = Path(run("gpgconf", "--list-dirs", "libexecdir", capture=True)) / "gpg-preset-passphrase"
    for grip in sorted(grips):
        # Send only over stdin to the host agent. Neither argv nor the container gets the password.
        run(preset, "--preset", "--restricted", grip, input=passphrase + "\n")


def sign_ci():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise ValueError("CI key import is only available in GitHub Actions; use distro-sign locally")
    if version() == "0.0.0" or os.environ.get("GITHUB_REF_NAME") != f"v{version()}":
        raise ValueError("CI signing requires an explicit non-placeholder release tag")
    secret = os.environ.pop("PERITUS_SIGNING_KEY", "")
    passphrase = os.environ.pop("PERITUS_SIGNING_PASSPHRASE", None)
    if not secret.startswith("-----BEGIN PGP PRIVATE KEY BLOCK-----") or passphrase is None:
        raise ValueError("configure the release-signing environment's key and passphrase secrets")
    if "\n" in passphrase or "\r" in passphrase:
        raise ValueError("the signing passphrase must be a single line")
    previous = os.environ.get("GNUPGHOME")
    with tempfile.TemporaryDirectory(prefix="peritus-ci-signing-") as temporary:
        home = Path(temporary)
        # This configuration belongs only to this disposable CI agent, never the user's agent.
        # Pinentry is unavailable on hosted runners; an unexpected prompt must fail immediately.
        (home / "gpg-agent.conf").write_text("allow-preset-passphrase\npinentry-program /bin/false\n")
        os.environ["GNUPGHOME"] = str(home)
        try:
            imported = subprocess.run(["gpg", "--batch", "--import"], text=True, input=secret,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                      check=False)
            secret = ""
            if imported.returncode:
                raise ValueError("CI signing key import failed; no key material is logged")
            key = fingerprint()
            primer = home / "agent-check.txt"
            primer.write_text("Peritus CI signing-agent preflight, not a release artifact.\n")
            run("gpg", "--batch", "--local-user", key, "--pinentry-mode", "loopback",
                "--passphrase-fd", "0", "--output", home / "agent-check.sig",
                "--detach-sign", primer, input=passphrase + "\n")
            seed_restricted_cache(key, passphrase)
            passphrase = ""
            sign()
        finally:
            try:
                run("gpgconf", "--kill", "gpg-agent")
            finally:
                if previous is None:
                    os.environ.pop("GNUPGHOME", None)
                else:
                    os.environ["GNUPGHOME"] = previous
