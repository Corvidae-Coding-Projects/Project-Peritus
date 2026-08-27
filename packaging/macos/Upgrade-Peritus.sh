#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: Upgrade-Peritus.sh <absolute-package-directory>" >&2
    exit 2
fi

bundle=$1
peritus_home=${HOME:?HOME is required}
app_root="$peritus_home/Library/Application Support/Peritus"
agent_file="$peritus_home/Library/LaunchAgents/com.corvidae.peritus.plist"
domain="gui/$(id -u)"
backup=$(mktemp -d "${TMPDIR:-/tmp}/peritus-upgrade.XXXXXXXX")
committed=0

restore() {
    if [ "$committed" -eq 0 ]; then
        launchctl bootout "$domain/com.corvidae.peritus" 2>/dev/null || true
        [ ! -d "$backup/bin" ] || { mkdir -p "$app_root/bin"; cp -p "$backup/bin/"* "$app_root/bin/" 2>/dev/null || true; }
        [ ! -d "$backup/libexec" ] || { mkdir -p "$app_root/libexec"; cp -p "$backup/libexec/"* "$app_root/libexec/" 2>/dev/null || true; }
        if [ -f "$backup/com.corvidae.peritus.plist" ]; then
            cp -p "$backup/com.corvidae.peritus.plist" "$agent_file"
            launchctl bootstrap "$domain" "$agent_file" 2>/dev/null || true
            launchctl kickstart -k "$domain/com.corvidae.peritus" 2>/dev/null || true
        fi
    fi
    rm -rf -- "$backup"
}
trap restore EXIT HUP INT TERM

[ ! -d "$app_root/bin" ] || cp -pR "$app_root/bin" "$backup/bin"
[ ! -d "$app_root/libexec" ] || cp -pR "$app_root/libexec" "$backup/libexec"
[ ! -f "$agent_file" ] || cp -p "$agent_file" "$backup/com.corvidae.peritus.plist"

launchctl bootout "$domain/com.corvidae.peritus" 2>/dev/null || true
"$bundle/Install-Peritus.sh" "$bundle"
committed=1
rm -rf -- "$backup"
trap - EXIT HUP INT TERM
