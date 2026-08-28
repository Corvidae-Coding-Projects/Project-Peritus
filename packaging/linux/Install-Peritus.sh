#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: Install-Peritus.sh <absolute-package-directory>" >&2
    exit 2
fi
bundle=$1
case "$bundle" in /*) ;; *) echo "package directory must be absolute" >&2; exit 2 ;; esac

peritus_user_home=${HOME:?HOME is required}
bin_root="$peritus_user_home/.local/bin"
helper_root="$peritus_user_home/.local/libexec/peritus"
share_root="$peritus_user_home/.local/share/peritus"
if [ ! -f "$bundle/SHA256SUMS" ] || [ ! -f "$bundle/manifest.toml" ]; then
    echo "package manifest and SHA256SUMS are required" >&2
    exit 2
fi
(cd "$bundle" && sha256sum --check --strict SHA256SUMS)

umask 077
install -d -m 700 "$bin_root" "$helper_root" "$share_root"
publish() {
    source_file=$1
    target_file=$2
    target_mode=$3
    temporary="$target_file.new.$$"
    install -m "$target_mode" "$source_file" "$temporary"
    mv -f "$temporary" "$target_file"
}
publish "$bundle/bin/peritusd" "$bin_root/peritusd" 755
publish "$bundle/bin/peritus" "$bin_root/peritus" 755
publish "$bundle/bin/peritus-tui" "$bin_root/peritus-tui" 755
publish "$bundle/libexec/peritus-linux-sandbox-helper" "$helper_root/peritus-linux-sandbox-helper" 755
publish "$bundle/share/peritus/peritus.service" "$share_root/peritus.service" 600

echo "Peritus installed. Start it with: peritus"
if ! command -v peritus >/dev/null 2>&1; then
    echo "Your shell does not currently search $bin_root; enable your desktop's standard user binary path and then run peritus." >&2
fi
