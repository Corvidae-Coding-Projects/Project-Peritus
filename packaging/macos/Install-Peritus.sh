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
app_root="$peritus_home/Library/Application Support/Peritus"
bin_root="$app_root/bin"
helper_root="$app_root/libexec"
config_file="$app_root/config/peritus.toml"
state_root="$app_root/state"
log_root="$peritus_home/Library/Logs/Peritus"
agent_file="$peritus_home/Library/LaunchAgents/com.corvidae.peritus.plist"
domain="gui/$(id -u)"

if [ ! -f "$config_file" ] || [ -L "$config_file" ]; then
    echo "operator-provisioned regular configuration is required at $config_file" >&2
    exit 2
fi
if [ ! -f "$bundle/SHA256SUMS" ] || [ ! -f "$bundle/manifest.toml" ]; then
    echo "package manifest and SHA256SUMS are required" >&2
    exit 2
fi
(cd "$bundle" && shasum -a 256 -c SHA256SUMS)

umask 077
install -d -m 700 "$bin_root" "$helper_root" "$state_root" "$log_root" "$(dirname "$agent_file")"
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
publish "$bundle/libexec/peritus-macos-sandbox-helper" "$helper_root/peritus-macos-sandbox-helper" 755

escape_sed() {
    printf '%s' "$1" | sed 's/[|&\\]/\\&/g'
}
template="$bundle/share/peritus/com.corvidae.peritus.plist.in"
temporary="$agent_file.new.$$"
sed \
    -e "s|@PERITUSD@|$(escape_sed "$bin_root/peritusd")|g" \
    -e "s|@CONFIG_FILE@|$(escape_sed "$config_file")|g" \
    -e "s|@STDOUT_LOG@|$(escape_sed "$log_root/peritusd.stdout.log")|g" \
    -e "s|@STDERR_LOG@|$(escape_sed "$log_root/peritusd.stderr.log")|g" \
    "$template" >"$temporary"
chmod 600 "$temporary"
plutil -lint "$temporary" >/dev/null
mv -f "$temporary" "$agent_file"

launchctl bootout "$domain/com.corvidae.peritus" 2>/dev/null || true
launchctl bootstrap "$domain" "$agent_file"
launchctl enable "$domain/com.corvidae.peritus"
launchctl kickstart -k "$domain/com.corvidae.peritus"

attempt=0
while [ "$attempt" -lt 30 ]; do
    if [ -f "$state_root/daemon.instance" ]; then
        endpoint_name=$(sed -n 's/^endpoint=//p' "$state_root/daemon.instance")
        if [ -n "$endpoint_name" ] && "$bin_root/peritus" --endpoint "$state_root/$endpoint_name.sock" status >/dev/null; then
            exit 0
        fi
    fi
    attempt=$((attempt + 1))
    sleep 1
done

echo "peritusd did not publish an authenticated ready endpoint" >&2
exit 1
