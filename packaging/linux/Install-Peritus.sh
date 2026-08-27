#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: Install-Peritus.sh <absolute-package-directory>" >&2
    exit 2
fi

bundle=$1
case "$bundle" in
    /*) ;;
    *) echo "package directory must be absolute" >&2; exit 2 ;;
esac

peritus_home=${HOME:?HOME is required}
config_root="$peritus_home/.config"
state_root="$peritus_home/.local/state"
bin_root="$peritus_home/.local/bin"
helper_root="$peritus_home/.local/libexec/peritus"
config_file="$config_root/peritus/peritus.toml"
unit_root="$config_root/systemd/user"
unit_file="$unit_root/peritus.service"
daemon_state="$state_root/peritus"

if [ ! -f "$config_file" ] || [ -L "$config_file" ]; then
    echo "operator-provisioned regular configuration is required at $config_file" >&2
    exit 2
fi
if [ ! -f "$bundle/SHA256SUMS" ] || [ ! -f "$bundle/manifest.toml" ]; then
    echo "package manifest and SHA256SUMS are required" >&2
    exit 2
fi
(cd "$bundle" && sha256sum --check --strict SHA256SUMS)

umask 077
install -d -m 700 "$bin_root" "$helper_root" "$unit_root" "$daemon_state" "$daemon_state/log"
chmod 600 "$config_file"

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
publish "$bundle/share/peritus/peritus.service" "$unit_file" 600

systemctl --user daemon-reload
systemctl --user enable --now peritus.service

attempt=0
while [ "$attempt" -lt 30 ]; do
    if [ -f "$daemon_state/daemon.instance" ]; then
        endpoint_name=$(sed -n 's/^endpoint=//p' "$daemon_state/daemon.instance")
        if [ -n "$endpoint_name" ] && "$bin_root/peritus" --endpoint "$daemon_state/$endpoint_name.sock" status >/dev/null; then
            exit 0
        fi
    fi
    attempt=$((attempt + 1))
    sleep 1
done

echo "peritusd did not publish an authenticated ready endpoint" >&2
exit 1
