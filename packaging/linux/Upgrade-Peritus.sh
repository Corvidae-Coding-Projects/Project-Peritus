#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then echo "usage: Upgrade-Peritus.sh <absolute-package-directory>" >&2; exit 2; fi
bundle=$1
peritus_user_home=${HOME:?HOME is required}
bin_root="$peritus_user_home/.local/bin"
helper_root="$peritus_user_home/.local/libexec/peritus"
share_root="$peritus_user_home/.local/share/peritus"
backup=$(mktemp -d "${TMPDIR:-/tmp}/peritus-upgrade.XXXXXXXX")
committed=0
restore() {
    if [ "$committed" -eq 0 ]; then
        [ ! -d "$backup/bin" ] || { mkdir -p "$bin_root"; cp -p "$backup/bin/"* "$bin_root/" 2>/dev/null || true; }
        [ ! -d "$backup/libexec" ] || { mkdir -p "$helper_root"; cp -p "$backup/libexec/"* "$helper_root/" 2>/dev/null || true; }
        [ ! -d "$backup/share" ] || { mkdir -p "$share_root"; cp -p "$backup/share/"* "$share_root/" 2>/dev/null || true; }
    fi
    rm -rf -- "$backup"
}
trap restore EXIT HUP INT TERM
[ ! -d "$bin_root" ] || cp -pR "$bin_root" "$backup/bin"
[ ! -d "$helper_root" ] || cp -pR "$helper_root" "$backup/libexec"
[ ! -d "$share_root" ] || cp -pR "$share_root" "$backup/share"
"$bundle/Install-Peritus.sh" "$bundle"
committed=1
rm -rf -- "$backup"
trap - EXIT HUP INT TERM
