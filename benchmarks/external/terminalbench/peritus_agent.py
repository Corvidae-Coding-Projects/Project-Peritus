"""Harbor custom-agent boundary for the native Peritus product runner."""

from __future__ import annotations

import hashlib
import json
import os
import shlex
import shutil
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any, override

from harbor.agents.base import BaseAgent
from harbor.environments.base import BaseEnvironment, ExecResult
from harbor.models.agent.context import AgentContext

from benchmarks.external.terminalbench.credential_state import (
    checkpoint_claude_credentials,
    credential_digest,
)
from benchmarks.external.terminalbench.deadline import product_budget_seconds
from benchmarks.external.terminalbench.process_supervisor import exec_supervised

_REPO_ROOT = Path(__file__).resolve().parents[3]
_PORTABLE_PERITUS = (
    _REPO_ROOT
    / "target/x86_64-unknown-linux-musl/release/peritus-benchmark-agent"
)
_REMOTE_BIN = PurePosixPath("/opt/peritus/bin")
_REMOTE_HOME = PurePosixPath("/tmp/peritus-home")
_REMOTE_PROMPTS = PurePosixPath("/tmp/peritus-prompts")


class PeritusAgent(BaseAgent):
    """Run Peritus itself inside each unchanged Harbor task environment."""

    def __init__(
        self,
        *args: Any,
        peritus_binary: str | None = None,
        codex_binary: str | None = None,
        codex_code_mode_host: str | None = None,
        claude_binary: str | None = None,
        codex_auth: str | None = None,
        claude_credentials: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(*args, **kwargs)
        self._peritus_binary = _required_file(
            peritus_binary or os.environ.get("PERITUS_BENCHMARK_AGENT"),
            _PORTABLE_PERITUS,
            "Peritus benchmark binary",
        )
        self._peritus_digest = _file_sha256(self._peritus_binary)
        self._codex_binary = _required_file(
            codex_binary or os.environ.get("PERITUS_CODEX_BINARY") or shutil.which("codex"),
            None,
            "Codex executable",
        )
        self._codex_code_mode_host = _required_file(
            codex_code_mode_host or os.environ.get("PERITUS_CODEX_CODE_MODE_HOST"),
            self._codex_binary.with_name("codex-code-mode-host"),
            "Codex code-mode host companion",
        )
        self._claude_binary = _required_file(
            claude_binary or os.environ.get("PERITUS_CLAUDE_BINARY") or shutil.which("claude"),
            None,
            "Claude executable",
        )
        self._codex_auth = _required_file(
            codex_auth or os.environ.get("CODEX_AUTH_JSON_PATH"),
            Path.home() / ".codex/auth.json",
            "Codex account state",
        )
        self._claude_credentials = _required_file(
            claude_credentials or os.environ.get("CLAUDE_CREDENTIALS_PATH"),
            Path.home() / ".claude/.credentials.json",
            "Claude account state",
        )
        self._uploaded_claude_digest: str | None = None
        self._run_number = 0

    @staticmethod
    @override
    def name() -> str:
        return "peritus"

    @override
    def version(self) -> str:
        return f"0.0.0+sha.{self._peritus_digest[:12]}"

    @override
    async def setup(self, environment: BaseEnvironment) -> None:
        platform = await environment.exec("uname -s && uname -m", user="root")
        _require_success(platform, "inspect task platform")
        if (platform.stdout or "").split() != ["Linux", "x86_64"]:
            raise RuntimeError(
                f"Peritus benchmark binaries require Linux x86_64, got {(platform.stdout or '').strip()!r}"
            )
        await self._ensure_git(environment)
        directories = " ".join(
            shlex.quote(str(path))
            for path in (
                _REMOTE_BIN,
                _REMOTE_HOME / ".codex",
                _REMOTE_HOME / ".claude",
                _REMOTE_PROMPTS,
                self.environment_logs_dir / "peritus",
            )
        )
        prepared = await environment.exec(f"mkdir -p {directories}", user="root")
        _require_success(prepared, "prepare Peritus runtime directories")

        self._uploaded_claude_digest = credential_digest(self._claude_credentials)
        uploads = (
            (self._peritus_binary, _REMOTE_BIN / "peritus-benchmark-agent"),
            (self._codex_binary, _REMOTE_BIN / "codex"),
            (self._codex_code_mode_host, _REMOTE_BIN / "codex-code-mode-host"),
            (self._claude_binary, _REMOTE_BIN / "claude"),
            (self._codex_auth, _REMOTE_HOME / ".codex/auth.json"),
            (self._claude_credentials, _REMOTE_HOME / ".claude/.credentials.json"),
        )
        for source, target in uploads:
            await environment.upload_file(source, str(target))

        owner = shlex.quote(str(environment.default_user or "root"))
        secured = await environment.exec(
            " && ".join(
                (
                    f"chmod 755 {shlex.quote(str(_REMOTE_BIN))}/*",
                    f"chmod 600 {shlex.quote(str(_REMOTE_HOME / '.codex/auth.json'))}",
                    f"chmod 600 {shlex.quote(str(_REMOTE_HOME / '.claude/.credentials.json'))}",
                    f"chown -R {owner} {shlex.quote(str(_REMOTE_HOME))} {shlex.quote(str(_REMOTE_PROMPTS))}",
                )
            ),
            user="root",
        )
        _require_success(secured, "secure Peritus runtime files")
        await self._qualify_runtime(environment)

    async def _ensure_git(self, environment: BaseEnvironment) -> None:
        command = """
if command -v git >/dev/null 2>&1; then exit 0; fi
if command -v apt-get >/dev/null 2>&1; then
  apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y git ca-certificates
elif command -v apk >/dev/null 2>&1; then
  apk add --no-cache git ca-certificates
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y git ca-certificates
elif command -v yum >/dev/null 2>&1; then
  yum install -y git ca-certificates
else
  echo 'no supported package manager can install git' >&2
  exit 1
fi
"""
        installed = await environment.exec(command, user="root", timeout_sec=300)
        _require_success(installed, "install Git in task environment")

    async def _qualify_runtime(self, environment: BaseEnvironment) -> None:
        runtime_env = await self._runtime_env(environment)
        protocol = await environment.exec(
            "peritus-benchmark-agent protocol", env=runtime_env, timeout_sec=30
        )
        _require_success(protocol, "qualify native Peritus protocol")
        _parse_protocol(protocol.stdout or "", self._peritus_digest)
        command = " && ".join(
            (
                "codex --version >/dev/null",
                "claude --version >/dev/null",
                "codex login status >/dev/null 2>&1",
                "claude auth status --json >/dev/null 2>&1",
            )
        )
        result = await environment.exec(command, env=runtime_env, timeout_sec=60)
        _require_success(result, "qualify authenticated provider routers")

    @override
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        self._run_number += 1
        workspace = await _workspace_path(environment)
        prompt_path = _REMOTE_PROMPTS / f"instruction-{self._run_number:04}.md"
        with tempfile.TemporaryDirectory(prefix="peritus-terminalbench-") as raw:
            local_prompt = Path(raw) / prompt_path.name
            local_prompt.write_text(instruction, encoding="utf-8")
            await environment.upload_file(local_prompt, str(prompt_path))

        session_id = str(self.context_id or self.session_id or "harbor-trial")
        task_id = environment.environment_name
        evidence_dir = self.environment_logs_dir / "peritus"
        max_elapsed_seconds = product_budget_seconds(self.logs_dir)
        command = " ".join(
            shlex.quote(value)
            for value in (
                "peritus-benchmark-agent",
                "terminalbench",
                "--workspace",
                str(workspace),
                "--evidence-dir",
                str(evidence_dir),
                "--prompt-file",
                str(prompt_path),
                "--session-id",
                session_id,
                "--task-id",
                task_id,
                "--model-id",
                self.model_name or "peritus-native",
                "--max-elapsed-seconds",
                str(max_elapsed_seconds),
            )
        )
        runtime_env = await self._runtime_env(environment)
        try:
            result = await exec_supervised(
                environment,
                command,
                pid_file=_REMOTE_HOME / f"peritus-run-{self._run_number:04}.pid",
                cwd=str(workspace),
                env=runtime_env,
            )
        finally:
            await self._checkpoint_claude_credentials(environment)
        self._retain_process_output(result)
        _require_success(result, "run native Peritus product composition")
        report = _parse_report(result.stdout or "")
        identity = _report_identity(report, self._peritus_digest)
        usage = report.get("usage") if isinstance(report.get("usage"), dict) else {}
        context.n_input_tokens = _integer(usage.get("input_tokens"))
        context.n_cache_tokens = _integer(usage.get("cached_input_tokens"))
        context.n_output_tokens = _integer(usage.get("output_tokens"))
        cost_microunits = _integer(usage.get("provider_cost_microunits"))
        if cost_microunits:
            context.cost_usd = cost_microunits / 1_000_000
        context.metadata = {
            "peritus_product_accepted": report.get("success") is True,
            "peritus_failure_kind": report.get("failure_kind"),
            "peritus_requests": _integer(usage.get("requests")) or 0,
            "peritus_source_revision": identity["source_revision"],
            "peritus_binary_sha256": identity["binary_sha256"],
        }

    async def _checkpoint_claude_credentials(self, environment: BaseEnvironment) -> None:
        uploaded_digest = self._uploaded_claude_digest
        if uploaded_digest is None:
            raise RuntimeError("Claude credential checkpoint has no uploaded-state identity")
        with tempfile.TemporaryDirectory(prefix="peritus-claude-state-") as raw:
            candidate = Path(raw) / ".credentials.json"
            await environment.download_file(
                str(_REMOTE_HOME / ".claude/.credentials.json"), candidate
            )
            checkpoint_claude_credentials(
                self._claude_credentials,
                uploaded_digest,
                candidate,
            )

    async def _runtime_env(self, environment: BaseEnvironment) -> dict[str, str]:
        return {
            "HOME": str(_REMOTE_HOME),
            "PATH": await _runtime_path(environment),
            "CARGO_BUILD_JOBS": "2",
        }

    def _retain_process_output(self, result: ExecResult) -> None:
        self.logs_dir.mkdir(parents=True, exist_ok=True)
        stem = f"peritus-run-{self._run_number:04}"
        (self.logs_dir / f"{stem}.stdout.json").write_text(
            result.stdout or "", encoding="utf-8"
        )
        (self.logs_dir / f"{stem}.stderr.log").write_text(
            result.stderr or "", encoding="utf-8"
        )


def _required_file(value: str | None, default: Path | None, label: str) -> Path:
    path = Path(value).expanduser() if value else default
    if path is None or not path.is_file():
        raise ValueError(f"{label} is unavailable; set its Peritus adapter path explicitly")
    return path.resolve()


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _require_success(result: ExecResult, operation: str) -> None:
    if result.return_code == 0:
        return
    detail = (result.stderr or result.stdout or "no diagnostic output").strip()
    raise RuntimeError(f"{operation} failed with exit {result.return_code}: {detail[:4000]}")


async def _workspace_path(environment: BaseEnvironment) -> PurePosixPath:
    result = await environment.exec("pwd -P")
    _require_success(result, "resolve task working directory")
    value = (result.stdout or "").strip()
    path = PurePosixPath(value)
    if (
        not value
        or "\n" in value
        or not path.is_absolute()
        or value != str(path)
        or path == PurePosixPath("/")
    ):
        raise RuntimeError(f"task working directory is not a safe absolute path: {value!r}")
    return path


async def _runtime_path(environment: BaseEnvironment) -> str:
    result = await environment.exec("printf '%s\\n' \"$PATH\"")
    _require_success(result, "resolve task executable path")
    value = (result.stdout or "").strip()
    segments = value.split(":")
    if (
        not value
        or "\n" in value
        or "\r" in value
        or any(not segment or not PurePosixPath(segment).is_absolute() for segment in segments)
    ):
        raise RuntimeError(f"task executable path is not a safe absolute path list: {value!r}")
    remote_bin = str(_REMOTE_BIN)
    return ":".join((remote_bin, *(segment for segment in segments if segment != remote_bin)))


def _parse_report(stdout: str) -> dict[str, Any]:
    malformed: json.JSONDecodeError | None = None
    for offset in range(len(stdout) - 1, -1, -1):
        if stdout[offset] != "{" or (offset > 0 and stdout[offset - 1] not in "\r\n"):
            continue
        try:
            report = json.loads(stdout[offset:])
        except json.JSONDecodeError as error:
            malformed = error
            continue
        if not isinstance(report, dict) or report.get("schema_version") != 2:
            raise RuntimeError("Peritus returned an unsupported Terminal-Bench report")
        return report
    raise RuntimeError("Peritus returned a malformed invocation report") from malformed


def _parse_protocol(stdout: str, expected_digest: str) -> dict[str, Any]:
    try:
        protocol = json.loads(stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("native Peritus protocol response is malformed") from error
    if (
        not isinstance(protocol, dict)
        or protocol.get("schema_version") != 1
        or protocol.get("terminalbench_report_schema_version") != 2
    ):
        raise RuntimeError("native Peritus executable and Harbor adapter are incompatible")
    _report_identity(protocol, expected_digest)
    return protocol


def _report_identity(report: dict[str, Any], expected_digest: str) -> dict[str, str]:
    identity = report.get("agent_identity")
    if not isinstance(identity, dict):
        raise RuntimeError("Peritus report has no native agent identity")
    package_version = identity.get("package_version")
    source_revision = identity.get("source_revision")
    binary_sha256 = identity.get("binary_sha256")
    if not isinstance(package_version, str) or not package_version:
        raise RuntimeError("Peritus report has no package version")
    if not isinstance(source_revision, str) or (
        len(source_revision) not in (40, 64)
        or any(character not in "0123456789abcdef" for character in source_revision)
    ):
        raise RuntimeError("Peritus report has no full source revision")
    if binary_sha256 != expected_digest:
        raise RuntimeError("Peritus report executable digest does not match the uploaded binary")
    return {
        "package_version": package_version,
        "source_revision": source_revision,
        "binary_sha256": binary_sha256,
    }


def _integer(value: object) -> int | None:
    return value if isinstance(value, int) and value >= 0 else None
