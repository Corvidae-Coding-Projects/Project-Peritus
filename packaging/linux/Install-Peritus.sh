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

fail() { printf '%s\n' "Peritus install failed: $*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
privileged() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@" </dev/null
    elif have sudo; then
        sudo -- "$@" </dev/null
    else
        fail "administrator access is required to install dependencies; install sudo or ask your administrator"
    fi
}

# Package managers retain ownership of dependencies. Never disable global security restrictions.
case "${PERITUS_INSTALL_DEPS:-1}" in 0|1) ;; *) fail "PERITUS_INSTALL_DEPS must be 0 or 1" ;; esac
missing=0
for dependency in git bwrap dbus-daemon; do
    have "$dependency" || missing=1
done
if ! have gnome-keyring-daemon && ! have kwalletd6 && ! have kwalletd5; then missing=1; fi
if [ "$missing" -eq 1 ]; then
    [ "${PERITUS_INSTALL_DEPS:-1}" = 1 ] ||
        fail "install Git, bubblewrap, D-Bus, and a Secret Service provider, then retry"
    printf '%s\n' "Installing Git, bubblewrap, D-Bus, a credential store, and CA certificates..."
    if have apt-get; then
        privileged apt-get update
        privileged apt-get install --yes --no-install-recommends git bubblewrap dbus-user-session gnome-keyring ca-certificates
    elif have dnf; then
        privileged dnf install --assumeyes git bubblewrap dbus-daemon gnome-keyring ca-certificates
    elif have pacman; then
        privileged pacman -S --needed --noconfirm git bubblewrap dbus gnome-keyring ca-certificates
    elif have zypper; then
        privileged zypper --non-interactive install git bubblewrap dbus-1 gnome-keyring ca-certificates
    else
        fail "no supported package manager found; install Git, bubblewrap, D-Bus, and a Secret Service provider, then retry"
    fi
fi
for dependency in git bwrap dbus-daemon; do
    have "$dependency" || fail "dependency installation did not provide $dependency"
done
if ! have gnome-keyring-daemon && ! have kwalletd6 && ! have kwalletd5; then
    fail "dependency installation did not provide a credential store"
fi
git --version >/dev/null || fail "Git does not run"
bwrap --version >/dev/null || fail "bubblewrap does not run"
sandbox_ready() {
    bwrap --unshare-user --unshare-pid --unshare-net --ro-bind / / --proc /proc --dev /dev -- /usr/bin/true </dev/null >/dev/null 2>&1
}
if ! sandbox_ready; then
    if [ "${PERITUS_INSTALL_DEPS:-1}" = 1 ] && have apt-get &&
        [ -r /sys/module/apparmor/parameters/enabled ] &&
        grep -q '^Y' /sys/module/apparmor/parameters/enabled; then
        printf '%s\n' "Installing the distribution's bubblewrap AppArmor profile..."
        privileged apt-get install --yes --no-install-recommends apparmor-profiles
        apparmor_profile=/usr/share/apparmor/extra-profiles/bwrap-userns-restrict
        [ -f "$apparmor_profile" ] || fail "the distribution did not supply its bubblewrap AppArmor profile"
        privileged apparmor_parser --replace "$apparmor_profile"
    fi
    sandbox_ready || fail "bubblewrap cannot create a sandbox; ask your administrator to enable its user namespaces and security profile"
fi
"$bundle/bin/peritus" --version >/dev/null ||
    fail "the package cannot run on this system; Linux release binaries require glibc 2.39 or later"

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
publish "$bundle/Uninstall-Peritus.sh" "$share_root/Uninstall-Peritus.sh" 700

# Append only the user binary path. Existing shell configuration stays intact.
case "${SHELL:-sh}" in
    */zsh) profile="$peritus_user_home/.zshrc" ;;
    */bash) profile="$peritus_user_home/.bashrc" ;;
    */fish) profile="${XDG_CONFIG_HOME:-$peritus_user_home/.config}/fish/conf.d/peritus.fish" ;;
    *) profile="$peritus_user_home/.profile" ;;
esac
path_line="export PATH=\"\$HOME/.local/bin:\$PATH\""
case "${SHELL:-sh}" in */fish) path_line="fish_add_path --path \"\$HOME/.local/bin\"" ;; esac
if [ -L "$profile" ]; then
    printf '%s\n' "Shell configuration is a symbolic link; add this line yourself: $path_line" >&2
else
    mkdir -p "$(dirname "$profile")"
    if ! grep -Fqx "$path_line" "$profile" 2>/dev/null; then
        printf '\n%s\n%s\n' '# Peritus user commands' "$path_line" >> "$profile"
    fi
fi
printf '%s\n' "Peritus installed. Open a new terminal and run: peritus"
printf '%s\n' "For this terminal, run: $bin_root/peritus"
printf '%s\n' "Linux tasks require kernel 6.6+, Landlock, and user namespaces. No global security restriction was disabled."
