#!/usr/bin/env python3
"""Build public consumers and replay scripted vendor turns for the rejected-turn diagnostic.

Seven scenarios run against a scripted OpenCode stand-in: one refused reply followed by a valid
corrected reply, two refused replies, a rate-limited turn followed by a valid reply, turns whose
plain prelude contains an inline code span or quoted pseudo-tool markup, a turn whose envelope
sits inside a `json` fence, and one refused reply through the developer loop. The provider
scenarios must publish the bounded
`peritus.opencode_turn_correction` extension carrying the rejected model text, or a
`peritus.response_healing` audit when the prelude is accepted; the agent scenarios must announce
their model request once and report the bounded retry. This is a scripted-vendor replay, not real
inference; it establishes the adapter retention, the healing contract, the diagnostic surface, and
the loop's public narration only.
"""
import argparse
import json
import os
import shutil
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", required=True, type=Path)
parser.add_argument(
    "--scenario",
    choices=(
        "all",
        "accepted",
        "rejected",
        "retry",
        "refused_turn",
        "prelude_backticks",
        "prelude_quoted",
        "prelude_fenced",
    ),
    default="all",
)
args = parser.parse_args()
here = Path(__file__).resolve().parent
checkout = here.parents[3]
root = args.output.resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)

project = root / "consumer"
(project / "src/bin").mkdir(parents=True)
shutil.copy2(here / "consumer.rs", project / "src/bin/provider.rs")
shutil.copy2(here / "agent.rs", project / "src/bin/agent.rs")
manifest = '[package]\nname="peritus-opencode-rejected-turn-replay"\nversion="0.1.0"\nedition="2024"\n[workspace]\n[dependencies]\n'
for package, path in (
    ("peritus-agent", "crates/orchestration/peritus-agent"),
    ("peritus-daemon", "crates/app/peritus-daemon"),
    ("peritus-model-protocol", "crates/model/peritus-model-protocol"),
    ("peritus-provider-core", "crates/model/peritus-provider-core"),
):
    manifest += package + "={path=" + json.dumps(str(checkout / path)) + "}\n"
manifest += 'tokio={version="=1.53.1",features=["rt","time","macros"]}\nserde_json="=1.0.149"\ntoml="=1.1.4"\n'
(project / "Cargo.toml").write_text(manifest)
shutil.copy2(checkout / "Cargo.lock", project / "Cargo.lock")
subprocess.run(
    [
        "cargo",
        "build",
        "--offline",
        "--jobs",
        "2",
        "--manifest-path",
        str(project / "Cargo.toml"),
        "--target-dir",
        str(checkout / "target"),
    ],
    check=True,
)

vendor = root / "scripted-opencode"
shutil.copy2(here / "scripted-opencode.py", vendor)
vendor.chmod(0o755)
(root / "tmp").mkdir()
(root / "key.txt").write_text("scripted-replay-placeholder-key\n")
(root / "config.toml").write_text(
    'version = 1\nstore_id = "00000000000000000000000000000001"\n\n'
    "[[providers]]\n"
    'kind = "hosted"\n'
    'hosted_service = "opencode-zen"\n'
    'wire_protocol = "opencode-runtime"\n'
    'credential_reference = "peritus-secret-v1:00000000000000000000000000000001:0000000000000000000000000000000000000000000000000000000000000001"\n\n'
    "[providers.profile]\n"
    'profile_id = "a8000000000000000000000000000001"\n'
    "revision = 1\n"
    'model = "scripted-reply-model"\n'
    'capabilities = ["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"]\n'
    "max_input_tokens = 32768\n"
    "max_output_tokens = 4096\n"
    "max_tools = 64\n"
    "max_parallel_tool_calls = 8\n"
    "max_inline_media_bytes = 1\n"
)

scenarios = {
    "accepted": {
        "binary": "provider",
        "script": ["invalid", "valid"],
        "required": ["TERMINAL succeeded", "RESULT accepted checks passed"],
    },
    "rejected": {
        "binary": "provider",
        "script": ["invalid", "invalid"],
        "required": [
            "TERMINAL failed MalformedPayload opencode.runtime.invalid_turn_json",
            "RESULT rejected checks passed",
        ],
    },
    "retry": {
        "binary": "agent",
        "script": ["rate_limit", "valid"],
        "required": [
            "COUNTS model_started=1 provider_retry=1 response_rejected=0",
            "ACTIVITY ProviderRetry 2/3 retryable_provider_response",
            "RESULT retry checks passed",
        ],
    },
    "refused_turn": {
        "binary": "agent",
        "script": ["invalid"],
        "expectation": "rejected",
        "required": [
            "COUNTS model_started=1 provider_retry=2 response_rejected=6",
            "RESULT rejected checks passed",
        ],
    },
    "prelude_backticks": {
        "binary": "provider",
        "script": ["prelude_backticks"],
        "required": [
            "HEALING private-envelope-v1 opencode.turn",
            "TERMINAL succeeded",
            "RESULT prelude_backticks checks passed",
        ],
    },
    "prelude_quoted": {
        "binary": "provider",
        "script": ["prelude_quoted"],
        "required": [
            "HEALING private-envelope-v1 opencode.turn",
            "TERMINAL succeeded",
            "RESULT prelude_quoted checks passed",
        ],
    },
    "prelude_fenced": {
        "binary": "provider",
        "script": ["prelude_fenced"],
        "required": [
            "HEALING private-envelope-v1 opencode.turn",
            "TERMINAL succeeded",
            "RESULT prelude_fenced checks passed",
        ],
    },
}
selected = (
    [args.scenario]
    if args.scenario != "all"
    else [
        "accepted",
        "rejected",
        "retry",
        "refused_turn",
        "prelude_backticks",
        "prelude_quoted",
        "prelude_fenced",
    ]
)
failures = []
for name in selected:
    scenario = scenarios[name]
    script = root / f"script-{name}.json"
    script.write_text(json.dumps(scenario["script"]))
    state = root / f"state-{name}"
    if state.exists():
        state.unlink()
    environment = os.environ.copy()
    environment.update(
        PERITUS_REAL_CASE_ROOT=str(root),
        PERITUS_REAL_CONFIG=str(root / "config.toml"),
        PERITUS_REAL_KEY_FILE=str(root / "key.txt"),
        PERITUS_REAL_CAPTURE=str(vendor),
        PERITUS_SCRIPT=str(script),
        PERITUS_SCRIPT_STATE=str(state),
        TMPDIR=str(root / "tmp"),
    )
    command = [
        str(checkout / "target/debug" / scenario["binary"]),
        scenario.get("expectation", name),
    ]
    transcript = root / f"transcript-{name}.txt"
    with transcript.open("w") as log:
        process = subprocess.run(
            command, env=environment, stdout=log, stderr=subprocess.STDOUT, text=True
        )
    observed = transcript.read_text()
    print(f"--- {name} (exit {process.returncode})")
    print(observed, end="")
    for required in scenario["required"]:
        if required not in observed:
            failures.append(f"{name}: missing {required!r}")
    if process.returncode != 0:
        failures.append(f"{name}: consumer exited {process.returncode}")
(root / "result.json").write_text(
    json.dumps(
        {
            "scenarios": selected,
            "failures": failures,
            "source_commit": subprocess.check_output(
                ["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True
            ).strip(),
        },
        indent=2,
    )
    + "\n"
)
if failures:
    raise SystemExit("; ".join(failures))
print("all scripted scenarios passed")
