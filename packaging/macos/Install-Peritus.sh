#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then echo "usage: Install-Peritus.sh <absolute-package-directory>" >&2; exit 2; fi
bundle=$1
case "$bundle" in /*) ;; *) echo "package directory must be absolute" >&2; exit 2 ;; esac
peritus_user_home=${HOME:?HOME is required}
app_root="$peritus_user_home/Library/Application Support/Peritus"
bin_root="$app_root/bin"
helper_root="$app_root/libexec"
share_root="$app_root/share/peritus"
command_root="$peritus_user_home/.local/bin"
if [ ! -f "$bundle/SHA256SUMS" ] || [ ! -f "$bundle/manifest.toml" ]; then echo "package manifest and SHA256SUMS are required" >&2; exit 2; fi
(cd "$bundle" && shasum -a 256 -c SHA256SUMS)
umask 077
install -d -m 700 "$bin_root" "$helper_root" "$share_root" "$command_root"
publish() { source_file=$1; target_file=$2; target_mode=$3; temporary="$target_file.new.$$"; install -m "$target_mode" "$source_file" "$temporary"; mv -f "$temporary" "$target_file"; }
publish "$bundle/bin/peritusd" "$bin_root/peritusd" 755
publish "$bundle/bin/peritus" "$bin_root/peritus" 755
publish "$bundle/bin/peritus-tui" "$bin_root/peritus-tui" 755
publish "$bundle/libexec/peritus-macos-sandbox-helper" "$helper_root/peritus-macos-sandbox-helper" 755
publish "$bundle/share/peritus/com.corvidae.peritus.plist.in" "$share_root/com.corvidae.peritus.plist.in" 600
ln -sfn "$bin_root/peritus" "$command_root/peritus"
echo "Peritus installed. Start it with: peritus"
if ! command -v peritus >/dev/null 2>&1; then echo "Your shell does not currently search $command_root; enable its standard user binary path and then run peritus." >&2; fi
