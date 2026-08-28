#!/bin/sh
set -eu
if [ "$#" -ne 1 ]; then echo "usage: Upgrade-Peritus.sh <absolute-package-directory>" >&2; exit 2; fi
bundle=$1
peritus_user_home=${HOME:?HOME is required}
app_root="$peritus_user_home/Library/Application Support/Peritus"
backup=$(mktemp -d "${TMPDIR:-/tmp}/peritus-upgrade.XXXXXXXX")
committed=0
restore() { if [ "$committed" -eq 0 ] && [ -d "$backup/Peritus" ]; then rm -rf -- "$app_root"; cp -pR "$backup/Peritus" "$app_root"; fi; rm -rf -- "$backup"; }
trap restore EXIT HUP INT TERM
[ ! -d "$app_root" ] || cp -pR "$app_root" "$backup/Peritus"
"$bundle/Install-Peritus.sh" "$bundle"
committed=1
rm -rf -- "$backup"
trap - EXIT HUP INT TERM
