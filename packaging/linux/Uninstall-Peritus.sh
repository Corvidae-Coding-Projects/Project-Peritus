#!/bin/sh
set -eu

if [ "$#" -ne 0 ]; then
    echo "usage: Uninstall-Peritus.sh" >&2
    exit 2
fi

peritus_home=${HOME:?HOME is required}
config_root="$peritus_home/.config"
bin_root="$peritus_home/.local/bin"
helper_root="$peritus_home/.local/libexec/peritus"
unit_file="$config_root/systemd/user/peritus.service"
pending_cleanup="$config_root/systemd/user/.peritus.service-removal-pending"
share_file="$peritus_home/.local/share/peritus/peritus.service"

had_registration=0
if [ -f "$unit_file" ] || [ -d "$pending_cleanup" ]; then
    if ! command -v systemctl >/dev/null 2>&1; then
        echo "systemctl is unavailable; preserving the registered service and package files" >&2
        exit 127
    fi
    load_state=$(systemctl --user show peritus.service --property=LoadState --value)
    if [ "$load_state" != "not-found" ]; then
        had_registration=1
        systemctl --user disable --now peritus.service
    fi
fi
if [ "$had_registration" -eq 1 ]; then
    if [ ! -d "$pending_cleanup" ]; then
        (umask 077 && mkdir -- "$pending_cleanup")
    fi
fi
rm -f -- "$unit_file"
if [ "$had_registration" -eq 1 ]; then
    systemctl --user daemon-reload
fi
if [ -d "$pending_cleanup" ]; then
    rmdir -- "$pending_cleanup"
fi
rm -f -- "$bin_root/peritusd" "$bin_root/peritus" "$bin_root/peritus-tui"
rm -f -- "$helper_root/peritus-linux-sandbox-helper"
rm -f -- "$share_file"
rm -f -- "$peritus_home/.local/share/peritus/Uninstall-Peritus.sh"
rmdir -- "$helper_root" 2>/dev/null || true
rmdir -- "$(dirname "$share_file")" 2>/dev/null || true

echo "Peritus package files were removed; configuration, state, logs, and credentials were preserved"
