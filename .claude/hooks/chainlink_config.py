#!/usr/bin/env python3
"""
Shared configuration and utilities for chainlink hooks.
Imported by other hook scripts to avoid code duplication.
"""

import json
import io
import os
import subprocess
import sys


def setup_utf8_stdout():
    """Fix Windows encoding issues with Unicode characters."""
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')


def _project_root_from_script():
    """Derive project root from this script's location (.claude/hooks/<script>.py -> project root)."""
    try:
        return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    except (NameError, OSError):
        return None


def get_project_root():
    """Get the project root directory.

    Prefers deriving from the hook script's own path (works even when cwd is a
    subdirectory), falling back to cwd.
    """
    root = _project_root_from_script()
    if root and os.path.isdir(root):
        return root
    return os.getcwd()


def find_chainlink_dir():
    """Find the .chainlink directory.

    Prefers the project root derived from the hook script's own path,
    falling back to walking up from cwd.
    """
    root = _project_root_from_script()
    if root:
        candidate = os.path.join(root, '.chainlink')
        if os.path.isdir(candidate):
            return candidate

    current = os.getcwd()
    for _ in range(10):
        candidate = os.path.join(current, '.chainlink')
        if os.path.isdir(candidate):
            return candidate
        parent = os.path.dirname(current)
        if parent == current:
            break
        current = parent
    return None


def run_chainlink(args, timeout=5):
    """Run a chainlink command and return output, or None on failure."""
    try:
        result = subprocess.run(
            ["chainlink"] + args,
            capture_output=True,
            text=True,
            timeout=timeout
        )
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, FileNotFoundError, Exception):
        return None


def run_command(cmd, timeout=5):
    """Run a shell command and return output, or None on failure."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            shell=True
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except (subprocess.TimeoutExpired, OSError, Exception):
        return None
    return None


def load_json_config(chainlink_dir, filename="hook-config.json"):
    """Load a JSON config file from the .chainlink directory. Returns dict or empty dict."""
    if not chainlink_dir:
        return {}
    config_path = os.path.join(chainlink_dir, filename)
    if not os.path.isfile(config_path):
        return {}
    try:
        with open(config_path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return {}


def load_tracking_mode(chainlink_dir):
    """Read tracking_mode from .chainlink/hook-config.json. Defaults to 'strict'."""
    config = load_json_config(chainlink_dir)
    mode = config.get("tracking_mode", "strict")
    if mode in ("strict", "normal", "relaxed"):
        return mode
    return "strict"


def load_guard_state(chainlink_dir):
    """Read drift tracking state from .chainlink/.cache/guard-state.json."""
    default = {
        "prompts_since_chainlink": 0,
        "total_prompts": 0,
        "last_chainlink_at": None,
        "last_reminder_at": None,
        "estimated_context_chars": 0,
        "context_budget_reinjections": 0,
    }
    if not chainlink_dir:
        return dict(default)
    state_path = os.path.join(chainlink_dir, ".cache", "guard-state.json")
    try:
        with open(state_path, "r", encoding="utf-8") as f:
            state = json.load(f)
        for key, val in default.items():
            state.setdefault(key, val)
        return state
    except (OSError, json.JSONDecodeError):
        return dict(default)


def save_guard_state(chainlink_dir, state):
    """Write drift tracking state to .chainlink/.cache/guard-state.json."""
    if not chainlink_dir:
        return
    cache_dir = os.path.join(chainlink_dir, ".cache")
    try:
        os.makedirs(cache_dir, exist_ok=True)
        state_path = os.path.join(cache_dir, "guard-state.json")
        with open(state_path, "w", encoding="utf-8") as f:
            json.dump(state, f)
    except OSError:
        return  # best-effort, don't block on write failure


def load_rule_file(rules_dir, filename, rules_local_dir=None):
    """Load a rule file, preferring rules.local/ override if present."""
    if not rules_dir:
        return ""
    # Check rules.local/ first for an override
    if rules_local_dir:
        local_path = os.path.join(rules_local_dir, filename)
        try:
            with open(local_path, 'r', encoding='utf-8') as f:
                return f.read().strip()
        except (OSError, IOError):
            pass  # local override not found, fall through to base rules
    # Fall back to rules/
    path = os.path.join(rules_dir, filename)
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return f.read().strip()
    except (OSError, IOError):
        return ""
