#!/usr/bin/env python3
"""
PreToolUse hook that blocks Write|Edit|Bash unless a chainlink issue
is being actively worked on. Forces issue creation before code changes.
"""

import json
import re
import subprocess
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from chainlink_config import (
    setup_utf8_stdout, find_chainlink_dir, run_chainlink, load_json_config,
)

setup_utf8_stdout()

# Defaults — overridden by .chainlink/hook-config.json if present
DEFAULT_BLOCKED_GIT = [
    "git push", "git commit", "git merge", "git rebase", "git cherry-pick",
    "git reset", "git checkout .", "git restore .", "git clean",
    "git stash", "git tag", "git am", "git apply",
    "git branch -d", "git branch -D", "git branch -m",
]

DEFAULT_ALLOWED_BASH = [
    "chainlink ",
    "git status", "git diff", "git log", "git branch", "git show",
    "cargo test", "cargo build", "cargo check", "cargo clippy", "cargo fmt",
    "npm test", "npm run", "npx ",
    "tsc", "node ", "python ",
    "ls", "dir", "pwd", "echo",
]


def load_config(chainlink_dir):
    """Load hook config from .chainlink/hook-config.json, falling back to defaults.

    Returns (tracking_mode, blocked_git, allowed_bash).
    tracking_mode is one of: "strict", "normal", "relaxed".
      strict  — block Write/Edit/Bash without an active issue
      normal  — remind (print warning) but don't block
      relaxed — no issue-tracking enforcement, only git blocks
    """
    blocked = list(DEFAULT_BLOCKED_GIT)
    allowed = list(DEFAULT_ALLOWED_BASH)
    mode = "strict"

    config = load_json_config(chainlink_dir)
    if not config:
        return mode, blocked, allowed

    if config.get("tracking_mode") in ("strict", "normal", "relaxed"):
        mode = config["tracking_mode"]
    if "blocked_git_commands" in config:
        blocked = config["blocked_git_commands"]
    if "allowed_bash_prefixes" in config:
        allowed = config["allowed_bash_prefixes"]

    return mode, blocked, allowed


def normalize_git_command(command):
    """Strip git global flags to extract the actual subcommand for matching.

    Git accepts flags like -C, --git-dir, --work-tree, -c before the
    subcommand. This normalizes 'git -C /path push' to 'git push' so
    that blocked command matching can't be bypassed.
    """
    import shlex

    try:
        parts = shlex.split(command)
    except ValueError:
        return command

    if not parts or parts[0] != "git":
        return command

    i = 1
    while i < len(parts):
        # Flags that take a separate next argument
        if parts[i] in ("-C", "--git-dir", "--work-tree", "-c") and i + 1 < len(parts):
            i += 2
        # Flags with =value syntax
        elif (
            parts[i].startswith("--git-dir=")
            or parts[i].startswith("--work-tree=")
        ):
            i += 1
        else:
            break

    if i < len(parts):
        return "git " + " ".join(parts[i:])
    return command


def is_blocked_git(input_data, blocked_list):
    """Check if a Bash command is a blocked git mutation. Always denied."""
    command = input_data.get("tool_input", {}).get("command", "").strip()
    normalized = normalize_git_command(command)
    for blocked in blocked_list:
        if normalized.startswith(blocked):
            return True
    # Also catch piped/chained git mutations: && git push, ; git commit, etc.
    for blocked in blocked_list:
        if f"&& {blocked}" in command or f"; {blocked}" in command or f"| {blocked}" in command:
            return True
    return False


def is_allowed_bash(input_data, allowed_list):
    """Check if a Bash command is on the allow list (read-only/infra)."""
    command = input_data.get("tool_input", {}).get("command", "").strip()
    normalized = normalize_git_command(command)
    for prefix in allowed_list:
        if normalized.startswith(prefix):
            return True
    return False


def is_claude_memory_path(input_data):
    """Check if a Write/Edit targets Claude Code's own memory/config directory (~/.claude/)."""
    file_path = input_data.get("tool_input", {}).get("file_path", "")
    if not file_path:
        return False
    home = os.path.expanduser("~")
    claude_dir = os.path.join(home, ".claude")
    try:
        return os.path.normcase(os.path.abspath(file_path)).startswith(
            os.path.normcase(os.path.abspath(claude_dir))
        )
    except (ValueError, OSError):
        return False


def main():
    try:
        input_data = json.load(sys.stdin)
        tool_name = input_data.get('tool_name', '')
    except (json.JSONDecodeError, Exception):
        tool_name = ''

    # Only check on Write, Edit, Bash
    if tool_name not in ('Write', 'Edit', 'Bash'):
        sys.exit(0)

    # Allow Claude Code to manage its own memory/config in ~/.claude/
    if tool_name in ('Write', 'Edit') and is_claude_memory_path(input_data):
        sys.exit(0)

    # PERMANENT BLOCK: prevent modification of hook infrastructure files
    if tool_name in ('Write', 'Edit'):
        file_path = input_data.get("tool_input", {}).get("file_path", "")
        norm = os.path.normcase(os.path.abspath(file_path)) if file_path else ""
        protected_patterns = [
            os.path.normcase("hook-config.json"),
            os.path.normcase(os.path.join(".claude", "hooks")),
            os.path.normcase(os.path.join(".claude", "settings.json")),
        ]
        if any(p in norm for p in protected_patterns):
            print(
                "MANDATORY COMPLIANCE — DO NOT MODIFY HOOK INFRASTRUCTURE.\n\n"
                "Editing hook scripts, hook-config.json, or .claude/settings.json is "
                "PERMANENTLY FORBIDDEN. These files control project safety constraints.\n\n"
                "You MUST NOT:\n"
                "  - Modify .claude/hooks/ files or .chainlink/hook-config.json\n"
                "  - Change tracking_mode, blocked_git_commands, or hook settings\n"
                "  - Edit .claude/settings.json to alter hook registration\n\n"
                "These are project-level safety controls managed by the human."
            )
            sys.exit(2)

    chainlink_dir = find_chainlink_dir()
    tracking_mode, blocked_git, allowed_bash = load_config(chainlink_dir)

    # PERMANENT BLOCK: git mutation commands are never allowed (all modes)
    if tool_name == 'Bash' and is_blocked_git(input_data, blocked_git):
        print(
            "MANDATORY COMPLIANCE — DO NOT ATTEMPT TO WORK AROUND THIS BLOCK.\n\n"
            "Git mutation commands (commit, push, merge, rebase, reset, etc.) are "
            "PERMANENTLY FORBIDDEN. The human performs all git write operations.\n\n"
            "You MUST NOT:\n"
            "  - Retry this command\n"
            "  - Rewrite the command to achieve the same effect\n"
            "  - Use a different tool to perform git mutations\n"
            "  - Ask the user if you should bypass this restriction\n\n"
            "You MUST instead:\n"
            "  - Inform the user that this is a manual step for them\n"
            "  - Continue with your other work\n\n"
            "Read-only git commands (status, diff, log, show, branch) are allowed."
        )
        sys.exit(2)

    # Allow read-only / infrastructure Bash commands through
    if tool_name == 'Bash' and is_allowed_bash(input_data, allowed_bash):
        sys.exit(0)

    # Relaxed mode: no issue-tracking enforcement
    if tracking_mode == "relaxed":
        sys.exit(0)

    if not chainlink_dir:
        sys.exit(0)

    # Check session status
    status = run_chainlink(["session", "status"])
    if not status:
        # chainlink not available — don't block
        sys.exit(0)

    # If already working on an issue, check lock status
    if "Working on: #" in status:
        # Extract issue ID from status
        match = re.search(r'Working on: #(\d+)', status)
        if match:
            issue_id = match.group(1)
            lock_result = run_chainlink(["locks", "check", issue_id])
            if lock_result and "locked by" in lock_result.lower():
                # Check if locked by another agent (not us)
                agent_json_path = os.path.join(chainlink_dir, "agent.json") if chainlink_dir else None
                our_agent_id = None
                if agent_json_path and os.path.isfile(agent_json_path):
                    try:
                        with open(agent_json_path, "r", encoding="utf-8") as f:
                            agent_data = json.load(f)
                            our_agent_id = agent_data.get("agent_id")
                    except (json.JSONDecodeError, OSError):
                        our_agent_id = None  # best-effort; treat as unknown agent

                if our_agent_id and our_agent_id in lock_result:
                    # Locked by us — allow
                    sys.exit(0)
                else:
                    lock_msg = (
                        f"Lock conflict: Issue #{issue_id} is {lock_result}\n\n"
                        "This issue is claimed by another agent. You should work on a different issue.\n"
                        "Use `chainlink locks list` to see all locks, or `chainlink next` to find available work."
                    )
                    if tracking_mode == "strict":
                        print(lock_msg)
                        sys.exit(2)
                    else:
                        # Normal/relaxed: warn but allow
                        print(f"Warning: {lock_msg}")
                        sys.exit(0)
        sys.exit(0)

    # No active work item — behavior depends on mode
    strict_msg = (
        "MANDATORY COMPLIANCE — DO NOT ATTEMPT TO WORK AROUND THIS BLOCK.\n\n"
        "You cannot Write, Edit, or run Bash commands without an active chainlink issue. "
        "This is a hard requirement enforced by the project, not a suggestion.\n\n"
        "You MUST NOT:\n"
        "  - Retry the same tool call hoping it will work\n"
        "  - Try alternative tools or approaches to bypass this check\n"
        "  - Proceed without creating an issue first\n"
        "  - Tell the user the hook is preventing you from working\n\n"
        "You MUST do exactly this, right now, before anything else:\n\n"
        "  chainlink quick \"<describe what you are about to do>\" -p <priority> -l <label>\n\n"
        "This single command creates an issue AND sets it as your active work item. "
        "After running it, your tool calls will succeed.\n\n"
        "If an issue already exists for this work, use:\n"
        "  chainlink list -s open\n"
        "  chainlink session work <id>\n\n"
        "This is how the project tracks work. Comply, then continue."
    )

    normal_msg = (
        "Reminder: No active chainlink issue. You should create one before making changes.\n\n"
        "  chainlink quick \"<describe what you are about to do>\" -p <priority> -l <label>\n\n"
        "Or pick an existing issue:\n"
        "  chainlink list -s open\n"
        "  chainlink session work <id>"
    )

    if tracking_mode == "strict":
        print(strict_msg)
        sys.exit(2)
    else:
        # normal mode: remind but allow
        print(normal_msg)
        sys.exit(0)


if __name__ == "__main__":
    main()
