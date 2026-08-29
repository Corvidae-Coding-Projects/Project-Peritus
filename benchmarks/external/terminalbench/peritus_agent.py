"""Harbor custom-agent boundary for the native Peritus product runner."""

from __future__ import annotations

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

_REPO_ROOT = Path(__file__).resolve().parents[3]
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
        claude_binary: str | None = None,
        codex_auth: str | None = None,
        claude_credentials: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(*args, **kwargs)
        self._peritus_binary = _required_file(
            peritus_binary or os.environ.get("PERITUS_BENCHMARK_AGENT"),
            _REPO_ROOT / "target/release/peritus-benchmark-agent",
            "Peritus benchmark binary",
        )
        self._codex_binary = _required_file(
            codex_binary or os.environ.get("PERITUS_CODEX_BINARY") or shutil.which("codex"),
            None,
            "Codex executable",
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
        self._run_number = 0

    @staticmethod
    @override
    def name() -> str:
        return "peritus"

    @override
    def version(self) -> str:
        return "0.0.0"

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

        uploads = (
            (self._peritus_binary, _REMOTE_BIN / "peritus-benchmark-agent"),
            (self._codex_binary, _REMOTE_BIN / "codex"),
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
        command = " && ".join(
            (
                "codex --version >/dev/null",
                "claude --version >/dev/null",
                "codex login status >/dev/null 2>&1",
                "claude auth status --json >/dev/null 2>&1",
            )
        )
        result = await environment.exec(command, env=self._runtime_env(), timeout_sec=60)
        _require_success(result, "qualify authenticated provider routers")

    @override
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        self._run_number += 1
        prompt_path = _REMOTE_PROMPTS / f"instruction-{self._run_number:04}.md"
        with tempfile.TemporaryDirectory(prefix="peritus-terminalbench-") as raw:
            local_prompt = Path(raw) / prompt_path.name
            local_prompt.write_text(instruction, encoding="utf-8")
            await environment.upload_file(local_prompt, str(prompt_path))

        session_id = str(self.context_id or self.session_id or "harbor-trial")
        task_id = environment.environment_name
        evidence_dir = self.environment_logs_dir / "peritus"
        command = " ".join(
            shlex.quote(value)
            for value in (
                "peritus-benchmark-agent",
                "terminalbench",
                "--workspace",
                "/app",
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
            )
        )
        result = await environment.exec(
            command,
            cwd="/app",
            env=self._runtime_env(),
        )
        self._retain_process_output(result)
        _require_success(result, "run native Peritus product composition")
        report = _parse_report(result.stdout or "")
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
        }

    def _runtime_env(self) -> dict[str, str]:
        return {
            "HOME": str(_REMOTE_HOME),
            "PATH": f"{_REMOTE_BIN}:/usr/local/bin:/usr/bin:/bin",
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


def _require_success(result: ExecResult, operation: str) -> None:
    if result.return_code == 0:
        return
    detail = (result.stderr or result.stdout or "no diagnostic output").strip()
    raise RuntimeError(f"{operation} failed with exit {result.return_code}: {detail[:4000]}")


def _parse_report(stdout: str) -> dict[str, Any]:
    try:
        report = json.loads(stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("Peritus returned a malformed invocation report") from error
    if not isinstance(report, dict) or report.get("schema_version") != 1:
        raise RuntimeError("Peritus returned an unsupported Terminal-Bench report")
    return report


def _integer(value: object) -> int | None:
    return value if isinstance(value, int) and value >= 0 else None
