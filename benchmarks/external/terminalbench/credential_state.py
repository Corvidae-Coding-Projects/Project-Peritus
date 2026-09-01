"""Safe host checkpointing for credential state rotated by official provider CLIs."""

from __future__ import annotations

import hashlib
import json
import os
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

_MAX_CREDENTIAL_BYTES = 64 * 1024
_MAX_TOKEN_BYTES = 32 * 1024


@dataclass(frozen=True)
class ClaudeCredentialState:
    """Non-secret ordering fields from one validated Claude credential document."""

    access_expires_at: int
    refresh_expires_at: int


def credential_digest(path: Path) -> str:
    """Hash one bounded credential document without exposing its contents."""

    return hashlib.sha256(_read_bounded(path)).hexdigest()


def checkpoint_claude_credentials(
    host_path: Path,
    uploaded_digest: str,
    candidate_path: Path,
    *,
    now_ms: int | None = None,
) -> bool:
    """Atomically retain a newer CLI-owned state if the uploaded host state is unchanged."""

    host_bytes = _read_bounded(host_path)
    if hashlib.sha256(host_bytes).hexdigest() != uploaded_digest:
        return False
    host = _decode(host_bytes)
    try:
        candidate_bytes = _read_bounded(candidate_path)
        candidate = _decode(candidate_bytes)
    except (OSError, ValueError):
        return False
    if candidate.access_expires_at < host.access_expires_at:
        return False
    current_time_ms = time.time_ns() // 1_000_000 if now_ms is None else now_ms
    if candidate.refresh_expires_at <= current_time_ms:
        return False
    if (
        candidate.refresh_expires_at < host.refresh_expires_at
        and candidate.access_expires_at <= host.access_expires_at
    ):
        return False
    if candidate_bytes == host_bytes:
        return False
    _atomic_replace(host_path, candidate_bytes)
    return True


def _decode(document: bytes) -> ClaudeCredentialState:
    try:
        value: Any = json.loads(document)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("Claude credential state is not valid JSON") from error
    if not isinstance(value, dict) or not isinstance(value.get("claudeAiOauth"), dict):
        raise ValueError("Claude credential state has no OAuth object")
    oauth = value["claudeAiOauth"]
    for field in ("accessToken", "refreshToken"):
        token = oauth.get(field)
        if not isinstance(token, str) or not token or len(token.encode("utf-8")) > _MAX_TOKEN_BYTES:
            raise ValueError(f"Claude credential field {field} is invalid")
    access_expires_at = oauth.get("expiresAt")
    refresh_expires_at = oauth.get("refreshTokenExpiresAt")
    if not isinstance(access_expires_at, int) or access_expires_at <= 0:
        raise ValueError("Claude access-token expiry is invalid")
    if not isinstance(refresh_expires_at, int) or refresh_expires_at <= 0:
        raise ValueError("Claude refresh-token expiry is invalid")
    return ClaudeCredentialState(access_expires_at, refresh_expires_at)


def _read_bounded(path: Path) -> bytes:
    with path.open("rb") as source:
        document = source.read(_MAX_CREDENTIAL_BYTES + 1)
    if not document or len(document) > _MAX_CREDENTIAL_BYTES:
        raise ValueError("Claude credential state is empty or exceeds its byte bound")
    return document


def _atomic_replace(path: Path, document: bytes) -> None:
    descriptor, raw_temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(raw_temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as target:
            descriptor = -1
            target.write(document)
            target.flush()
            os.fsync(target.fileno())
        os.replace(temporary, path)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        temporary.unlink(missing_ok=True)
