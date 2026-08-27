#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: Upgrade-Peritus.sh <absolute-package-directory>" >&2
    exit 2
fi

bundle=$1
peritus_home=${HOME:?HOME is required}
config_root="$peritus_home/.config"
bin_root="$peritus_home/.local/bin"
helper_root="$peritus_home/.local/libexec/peritus"
unit_file="$config_root/systemd/user/peritus.service"
backup=$(mktemp -d "${TMPDIR:-/tmp}/peritus-upgrade.XXXXXXXX")
committed=0

restore() {
    if [ "$committed" -eq 0 ]; then
        systemctl --user stop peritus.service 2>/dev/null || true
        for relative in bin/peritusd bin/peritus bin/peritus-tui libexec/peritus-linux-sandbox-helper share/peritus/peritus.service; do
            if [ -f "$backup/$relative" ]; then
                case "$relative" in
                    bin/*) target="$bin_root/${relative#bin/}" ;;
                    libexec/*) target="$helper_root/peritus-linux-sandbox-helper" ;;
                    share/*) target="$unit_file" ;;
                esac
                cp -p "$backup/$relative" "$target"
            fi
        done
        systemctl --user daemon-reload 2>/dev/null || true
        systemctl --user start peritus.service 2>/dev/null || true
    fi
    rm -rf -- "$backup"
}
trap restore EXIT HUP INT TERM

mkdir -p "$backup/bin" "$backup/libexec" "$backup/share/peritus"
[ ! -f "$bin_root/peritusd" ] || cp -p "$bin_root/peritusd" "$backup/bin/peritusd"
[ ! -f "$bin_root/peritus" ] || cp -p "$bin_root/peritus" "$backup/bin/peritus"
[ ! -f "$bin_root/peritus-tui" ] || cp -p "$bin_root/peritus-tui" "$backup/bin/peritus-tui"
[ ! -f "$helper_root/peritus-linux-sandbox-helper" ] || cp -p "$helper_root/peritus-linux-sandbox-helper" "$backup/libexec/peritus-linux-sandbox-helper"
[ ! -f "$unit_file" ] || cp -p "$unit_file" "$backup/share/peritus/peritus.service"

systemctl --user stop peritus.service
"$bundle/Install-Peritus.sh" "$bundle"
committed=1
rm -rf -- "$backup"
trap - EXIT HUP INT TERM
