"""Credential-lifecycle regressions for the Terminal-Bench adapter."""

from __future__ import annotations

import json
import stat
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from benchmarks.external.terminalbench.credential_state import (
    checkpoint_claude_credentials,
    checkpoint_claude_credentials_and_advance,
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

            changed = checkpoint_claude_credentials(
                host, uploaded, candidate, now_ms=50
            )

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

            changed = checkpoint_claude_credentials(
                host, uploaded, candidate, now_ms=50
            )

            self.assertFalse(changed)
            self.assertEqual(host.read_bytes(), _document(300, 1_000, "other-owner"))

    def test_preserves_host_for_invalid_candidate_state(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            original = _document(200, 1_000, "host")
            host.write_bytes(original)
            uploaded = credential_digest(host)
            for value in (b"not-json", b""):
                with self.subTest(value=value):
                    candidate.write_bytes(value)

                    changed = checkpoint_claude_credentials(
                        host, uploaded, candidate, now_ms=50
                    )

                    self.assertFalse(changed)
                    self.assertEqual(host.read_bytes(), original)

    def test_preserves_host_when_candidate_reduces_either_lifetime(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            for access, refresh in ((100, 1_000), (200, 900)):
                with self.subTest(access=access, refresh=refresh):
                    host = root / "credentials.json"
                    candidate = root / "candidate.json"
                    original = _document(200, 1_000, "host")
                    host.write_bytes(original)
                    candidate.write_bytes(_document(access, refresh, "older"))

                    changed = checkpoint_claude_credentials(
                        host, credential_digest(host), candidate, now_ms=50
                    )

                    self.assertFalse(changed)
                    self.assertEqual(host.read_bytes(), original)

    def test_rejects_invalid_host_state(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            host.write_bytes(b"not-json")
            candidate.write_bytes(_document(200, 1_000, "candidate"))

            with self.assertRaises(ValueError):
                checkpoint_claude_credentials(
                    host, credential_digest(host), candidate, now_ms=50
                )

    def test_retains_rotation_with_newer_access_and_shorter_refresh_lifetime(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            host.write_bytes(_document(200, 2_000, "old"))
            candidate.write_bytes(_document(300, 1_000, "rotated"))

            changed = checkpoint_claude_credentials(
                host, credential_digest(host), candidate, now_ms=50
            )

            self.assertTrue(changed)
            self.assertEqual(host.read_bytes(), candidate.read_bytes())

    def test_rejects_rotation_with_expired_refresh_credential(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            original = _document(200, 2_000, "host")
            host.write_bytes(original)
            candidate.write_bytes(_document(300, 500, "expired"))

            changed = checkpoint_claude_credentials(
                host, credential_digest(host), candidate, now_ms=500
            )

            self.assertFalse(changed)
            self.assertEqual(host.read_bytes(), original)

    def test_successive_cli_rotations_advance_the_expected_host_identity(self) -> None:
        with TemporaryDirectory() as raw:
            root = Path(raw)
            host = root / "credentials.json"
            candidate = root / "candidate.json"
            host.write_bytes(_document(100, 3_000, "initial"))
            expected = credential_digest(host)

            candidate.write_bytes(_document(200, 3_000, "first"))
            expected = checkpoint_claude_credentials_and_advance(
                host, expected, candidate, now_ms=50
            )
            self.assertEqual(expected, credential_digest(host))

            candidate.write_bytes(_document(300, 3_000, "second"))
            expected = checkpoint_claude_credentials_and_advance(
                host, expected, candidate, now_ms=50
            )

            self.assertEqual(expected, credential_digest(host))
            self.assertEqual(host.read_bytes(), candidate.read_bytes())


if __name__ == "__main__":
    unittest.main()
