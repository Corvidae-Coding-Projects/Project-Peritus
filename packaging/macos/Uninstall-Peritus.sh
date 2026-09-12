#!/bin/sh
set -eu

if [ "$#" -ne 0 ]; then
    echo "usage: Uninstall-Peritus.sh" >&2
    exit 2
fi

peritus_home=${HOME:?HOME is required}
app_root="$peritus_home/Library/Application Support/Peritus"
agent_file="$peritus_home/Library/LaunchAgents/com.corvidae.peritus.plist"
domain="gui/$(id -u)"
command_file="$peritus_home/.local/bin/peritus"

if command -v launchctl >/dev/null 2>&1; then
    # Validate access to the user's launchd domain separately from an absent job.
    launchctl print "$domain" >/dev/null
    if launchctl print "$domain/com.corvidae.peritus" >/dev/null 2>&1; then
        launchctl bootout "$domain/com.corvidae.peritus"
    else
        status=$?
        # launchctl reports ESRCH when the domain is available but the job is absent.
        [ "$status" -eq 113 ] || exit "$status"
    fi
elif [ -f "$agent_file" ]; then
    echo "launchctl is unavailable; preserving the registered agent and package files" >&2
    exit 127
fi
rm -f -- "$agent_file"
rm -f -- "$command_file"
rm -f -- "$app_root/bin/peritusd" "$app_root/bin/peritus" "$app_root/bin/peritus-tui"
rm -f -- "$app_root/libexec/peritus-macos-sandbox-helper"
rm -f -- "$app_root/share/peritus/com.corvidae.peritus.plist.in"
rm -f -- "$app_root/share/peritus/Uninstall-Peritus.sh"
rmdir -- "$app_root/bin" "$app_root/libexec" "$app_root/share/peritus" "$app_root/share" 2>/dev/null || true

echo "Peritus package files were removed; configuration, state, logs, and credentials were preserved"
