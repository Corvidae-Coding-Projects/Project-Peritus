"""Credential-lifecycle regressions for the Terminal-Bench adapter."""

from __future__ import annotations

import json
import stat
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from benchmarks.external.terminalbench.credential_state import (
    checkpoint_claude_credentials,
    credential_digest,
)


def _document(access: int, refresh: int, suffix: str) -> bytes:
    return json.dumps(
        {
            "claudeAiOauth": {
                "accessToken": f"access-{suffix}",
                "refreshToken": f"refresh-{suffix}",
                "expiresAt": access,
                "refreshTokenExpiresAt": refresh,
            }
        },
        separators=(",", ":"),
    ).encode()


class CredentialCheckpointTests(unittest.TestCase):
    def test_retains_newer_cli_owned_state_atomically_with_private_mode(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            host.write_bytes(_document(100, 1_000, "old"))
            candidate.write_bytes(_document(200, 1_000, "new"))
            uploaded = credential_digest(host)

            changed = checkpoint_claude_credentials(host, uploaded, candidate)

            self.assertTrue(changed)
            self.assertEqual(host.read_bytes(), candidate.read_bytes())
            self.assertEqual(stat.S_IMODE(host.stat().st_mode), 0o600)

    def test_does_not_overwrite_host_state_changed_by_another_serial_owner(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            host.write_bytes(_document(100, 1_000, "uploaded"))
            uploaded = credential_digest(host)
            host.write_bytes(_document(300, 1_000, "other-owner"))
            candidate.write_bytes(_document(200, 1_000, "container"))

            changed = checkpoint_claude_credentials(host, uploaded, candidate)

            self.assertFalse(changed)
            self.assertEqual(host.read_bytes(), _document(300, 1_000, "other-owner"))

    def test_rejects_invalid_or_rollback_state(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            host.write_bytes(_document(200, 1_000, "host"))
            uploaded = credential_digest(host)
            for value in (b"not-json", _document(100, 1_000, "older")):
                with self.subTest(value=value[:12]):
                    candidate.write_bytes(value)
                    with self.assertRaises(ValueError):
                        checkpoint_claude_credentials(host, uploaded, candidate)


if __name__ == "__main__":
    unittest.main()
