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
share_file="$peritus_home/.local/share/peritus/peritus.service"

systemctl --user disable --now peritus.service 2>/dev/null || true
rm -f -- "$unit_file"
systemctl --user daemon-reload 2>/dev/null || true
rm -f -- "$bin_root/peritusd" "$bin_root/peritus" "$bin_root/peritus-tui"
rm -f -- "$helper_root/peritus-linux-sandbox-helper"
rm -f -- "$share_file"
rmdir -- "$helper_root" 2>/dev/null || true
rmdir -- "$(dirname "$share_file")" 2>/dev/null || true

echo "Peritus package files were removed; configuration, state, logs, and credentials were preserved"
