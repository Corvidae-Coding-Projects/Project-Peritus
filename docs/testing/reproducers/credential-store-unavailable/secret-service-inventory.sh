#!/usr/bin/env bash
# Asks the session Secret Service which Peritus provider credentials exist, and can remove them.
#
# The repository sandbox denies socket creation, so run this outside it. Requires dbus-daemon,
# gnome-keyring-daemon, and busctl. Without --remove nothing is deleted.
set -u

remove=0
if [ "${1:-}" = "--remove" ]; then
    remove=1
fi

bus="$(dbus-daemon --session --fork --print-address=1 --print-pid=1)"
export DBUS_SESSION_BUS_ADDRESS="${bus%%$'\n'*}"
export DBUS_SESSION_BUS_PID="${bus##*$'\n'}"
printf '\n' | gnome-keyring-daemon --daemonize --unlock --components=secrets >/dev/null 2>&1
sleep 1

search() {
    busctl --user call org.freedesktop.secrets /org/freedesktop/secrets \
        org.freedesktop.Secret.Service SearchItems 'a{ss}' 1 \
        service org.corvidae-coding.peritus.providers
}

echo "--- items in the Peritus provider namespace:"
reply="$(search)"
echo "$reply"

if [ "$remove" = 1 ]; then
    for path in $(printf '%s' "$reply" | tr ' ' '\n' | grep '^"/org/freedesktop/secrets/collection/' | tr -d '"'); do
        echo "--- deleting $path"
        busctl --user call org.freedesktop.secrets "$path" org.freedesktop.Secret.Item Delete
    done
    echo "--- items after removal:"
    search
fi
