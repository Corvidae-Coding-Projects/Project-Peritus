"""Explicit CI-only import into an ephemeral keyring; local signing never exports a key."""

import os
from pathlib import Path
import subprocess
import tempfile

from common import run, version
from sign import fingerprint, sign


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
            passphrase = ""
            sign()
        finally:
            run("gpgconf", "--kill", "gpg-agent")
            if previous is None:
                os.environ.pop("GNUPGHOME", None)
            else:
                os.environ["GNUPGHOME"] = previous
