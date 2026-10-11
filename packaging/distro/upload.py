"""Upload only a verified, exact-commit distribution package set to an existing draft."""

import json
import os

from common import ROOT, architecture, package_format, run, version
from verify import verify


def upload():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise ValueError("automatic upload is only supported by the tagged release workflow")
    tag = os.environ.get("GITHUB_REF_NAME")
    if version() == "0.0.0" or tag != f"v{version()}":
        raise ValueError("package upload requires the exact non-placeholder release tag")
    kind = package_format()
    directory = ROOT / "dist" / "signed" / kind
    record = json.loads((directory / "packages" / f"peritus-{kind}-build.json").read_text())
    if record["git_commit"] != os.environ.get("GITHUB_SHA"):
        raise ValueError("package upload does not match the tagged release commit")
    release = json.loads(run("gh", "release", "view", tag, "--json", "tagName,isDraft", capture=True))
    if release != {"tagName": tag, "isDraft": True}:
        raise ValueError("distribution packages may only be uploaded to the exact existing draft")
    verify()
    # One matrix leg owns the shared key asset; concurrent --clobber uploads
    # of the same name can race even when their public-key bytes are identical.
    owns_key = kind == "deb" and architecture() == "x86_64"
    assets = sorted(path for path in (directory / "assets").iterdir()
                    if owns_key or path.name != "peritus-release.asc")
    run("gh", "release", "upload", tag, "--clobber", *assets)
