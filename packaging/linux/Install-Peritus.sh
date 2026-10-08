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

preinstall_root="$share_root/.install"
preinstall_transactions="$preinstall_root/transactions"
install -d -m 700 "$share_root" "$preinstall_root" "$preinstall_transactions"
if [ "${PERITUS_INSTALL_LOCKED:-0}" != 1 ]; then
    exec "$bundle/bin/peritusd" package-lock --lock "$preinstall_transactions/install.lock" -- \
        "$bundle/Install-Peritus.sh" "$bundle"
fi

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
install_root="$share_root/.install"
generation_root="$install_root/generations"
transaction_root="$install_root/transactions"
receipt_root="$install_root/receipts"
current="$install_root/current"
active="$transaction_root/active"
migration="$transaction_root/migration"
install -d -m 700 "$bin_root" "$helper_root" "$share_root" \
    "$install_root" "$generation_root" "$transaction_root" "$receipt_root"

hash_file() { sha256sum "$1" | { read -r digest ignored; printf '%s\n' "$digest"; }; }
generation=$(hash_file "$bundle/SHA256SUMS")

record() {
    destination=$1
    shift
    temporary="$destination.new.$$"
    { for line in "$@"; do printf '%s\n' "$line"; done; } > "$temporary"
    sync
    mv -f "$temporary" "$destination"
    sync
}

field() {
    key=$1
    source=$2
    value=$(sed -n "s/^${key}=//p" "$source")
    [ -n "$value" ] || fail "durable install receipt is missing $key"
    [ "$(printf '%s\n' "$value" | wc -l)" -eq 1 ] || fail "durable install receipt repeats $key"
    printf '%s\n' "$value"
}

stage_generation() {
    identifier=$1
    destination="$generation_root/$identifier"
    if [ -d "$destination" ]; then
        verify_generation "$identifier" || fail "retained generation $identifier is incomplete"
        return
    fi
    temporary="$generation_root/.stage-$identifier-$$"
    rm -rf -- "$temporary"
    install -d -m 700 "$temporary/bin" "$temporary/libexec" "$temporary/share/peritus"
    install -m 755 "$bundle/bin/peritusd" "$temporary/bin/peritusd"
    install -m 755 "$bundle/bin/peritus" "$temporary/bin/peritus"
    install -m 755 "$bundle/bin/peritus-tui" "$temporary/bin/peritus-tui"
    install -m 755 "$bundle/libexec/peritus-linux-sandbox-helper" \
        "$temporary/libexec/peritus-linux-sandbox-helper"
    install -m 600 "$bundle/share/peritus/peritus.service" "$temporary/share/peritus/peritus.service"
    install -m 700 "$bundle/Install-Peritus.sh" "$temporary/Install-Peritus.sh"
    install -m 700 "$bundle/Upgrade-Peritus.sh" "$temporary/Upgrade-Peritus.sh"
    install -m 700 "$bundle/Uninstall-Peritus.sh" "$temporary/Uninstall-Peritus.sh"
    install -m 600 "$bundle/manifest.toml" "$temporary/manifest.toml"
    install -m 600 "$bundle/SHA256SUMS" "$temporary/SHA256SUMS"
    printf '%s\n' "$identifier" > "$temporary/.source-sha256"
    sync
    mv "$temporary" "$destination"
    sync
}

verify_generation() {
    identifier=$1
    root="$generation_root/$identifier"
    [ -f "$root/.source-sha256" ] && [ "$(cat "$root/.source-sha256")" = "$identifier" ] &&
        cmp -s "$root/bin/peritusd" "$bundle/bin/peritusd" &&
        cmp -s "$root/bin/peritus" "$bundle/bin/peritus" &&
        cmp -s "$root/bin/peritus-tui" "$bundle/bin/peritus-tui" &&
        cmp -s "$root/libexec/peritus-linux-sandbox-helper" "$bundle/libexec/peritus-linux-sandbox-helper" &&
        cmp -s "$root/share/peritus/peritus.service" "$bundle/share/peritus/peritus.service" &&
        cmp -s "$root/Uninstall-Peritus.sh" "$bundle/Uninstall-Peritus.sh"
}

verify_retained_generation() {
    identifier=$1
    root="$generation_root/$identifier"
    [ -f "$root/.source-sha256" ] && [ "$(cat "$root/.source-sha256")" = "$identifier" ] &&
        [ -x "$root/bin/peritusd" ] && [ -x "$root/bin/peritus" ] &&
        [ -x "$root/bin/peritus-tui" ] &&
        [ -x "$root/libexec/peritus-linux-sandbox-helper" ] &&
        [ -f "$root/share/peritus/peritus.service" ] && [ -x "$root/Uninstall-Peritus.sh" ] ||
        return 1
    case "$identifier" in
        legacy-*) return 0 ;;
        *) (cd "$root" && sha256sum --check --strict SHA256SUMS >/dev/null) ;;
    esac
}

configuration_from_marker() {
    marker="${XDG_STATE_HOME:-$peritus_user_home/.local/state}/peritus/daemon/applied-configuration"
    if [ -f "$marker" ]; then
        [ "$(sed -n '1p' "$marker")" = "peritus-applied-daemon-v2" ] ||
            fail "applied daemon configuration marker is malformed"
        value=$(sed -n 's/^configuration=//p' "$marker")
        [ -n "$value" ] || fail "applied daemon configuration marker has no configuration"
        [ "$(printf '%s\n' "$value" | wc -l)" -eq 1 ] ||
            fail "applied daemon configuration marker repeats configuration"
        printf '%s\n' "$value"
    elif [ -n "${PERITUS_DAEMON_CONFIG:-}" ]; then
        printf '%s\n' "$PERITUS_DAEMON_CONFIG"
    elif [ -f "${XDG_CONFIG_HOME:-$peritus_user_home/.config}/peritus/peritus.toml" ]; then
        printf '%s\n' "${XDG_CONFIG_HOME:-$peritus_user_home/.config}/peritus/peritus.toml"
    fi
}

configuration_for_transaction() {
    value=$(configuration_from_marker)
    if [ -z "$value" ]; then
        printf '%s\n' none
        return
    fi
    case "$value" in
        *'
'*) fail "daemon configuration path contains a line break" ;;
    esac
    case "$value" in
        /*) ;;
        *) value="$(pwd -P)/$value" ;;
    esac
    [ -f "$value" ] || fail "recorded daemon configuration does not exist: $value"
    printf '%s\n' "$value"
}

validate_transaction_configuration() {
    case "$1" in
        none) ;;
        /*) [ -f "$1" ] || fail "recorded daemon configuration does not exist: $1" ;;
        *) fail "durable install receipt has a non-absolute configuration" ;;
    esac
}

atomic_current() {
    identifier=$1
    configuration=$2
    temporary="$install_root/.current.$$"
    rm -f -- "$temporary"
    ln -s "generations/$identifier" "$temporary"
    if [ "$configuration" != none ]; then
        adopted=0
        "$bundle/bin/peritusd" package-adopt --store "$install_root" \
            --candidate "$temporary" --current "$current" --config "$configuration" && adopted=1
    else
        adopted=0
        "$bundle/bin/peritusd" package-adopt --store "$install_root" \
            --candidate "$temporary" --current "$current" && adopted=1
    fi
    if [ "$adopted" -ne 1 ]; then
        rm -f -- "$temporary"
        fail "current generation could not be adopted atomically"
    fi
    sync
}

stable_link() {
    stable_target=$1
    stable_path=$2
    temporary="$stable_path.new.$$"
    rm -f -- "$temporary"
    ln -s "$stable_target" "$temporary"
    mv -f "$temporary" "$stable_path"
}

ensure_stable_links() {
    stable_link "$current/bin/peritusd" "$bin_root/peritusd"
    stable_link "$current/bin/peritus" "$bin_root/peritus"
    stable_link "$current/bin/peritus-tui" "$bin_root/peritus-tui"
    stable_link "$current/libexec/peritus-linux-sandbox-helper" \
        "$helper_root/peritus-linux-sandbox-helper"
    stable_link "$current/share/peritus/peritus.service" "$share_root/peritus.service"
    stable_link "$current/Uninstall-Peritus.sh" "$share_root/Uninstall-Peritus.sh"
    sync
}

verify_stable_links() {
    [ -x "$bin_root/peritusd" ] && [ -x "$bin_root/peritus" ] && [ -x "$bin_root/peritus-tui" ] &&
        [ -x "$helper_root/peritus-linux-sandbox-helper" ] &&
        [ -f "$share_root/peritus.service" ] && [ -x "$share_root/Uninstall-Peritus.sh" ]
}

remove_stable_links() {
    for pair in \
        "$bin_root/peritusd|$current/bin/peritusd" \
        "$bin_root/peritus|$current/bin/peritus" \
        "$bin_root/peritus-tui|$current/bin/peritus-tui" \
        "$helper_root/peritus-linux-sandbox-helper|$current/libexec/peritus-linux-sandbox-helper" \
        "$share_root/peritus.service|$current/share/peritus/peritus.service" \
        "$share_root/Uninstall-Peritus.sh|$current/Uninstall-Peritus.sh"
    do
        link=${pair%%|*}
        expected=${pair#*|}
        if [ -L "$link" ]; then
            [ "$(readlink "$link")" = "$expected" ] ||
                fail "refusing to remove an unexpected stable package link: $link"
            rm -f -- "$link"
        elif [ -e "$link" ]; then
            fail "refusing to remove a non-link stable package path: $link"
        fi
    done
    sync
}

is_hex64() {
    [ "${#1}" -eq 64 ] || return 1
    case "$1" in *[!0-9a-f]*) return 1 ;; *) return 0 ;; esac
}

stage_legacy_generation() {
    for path in "$bin_root/peritusd" "$bin_root/peritus" "$bin_root/peritus-tui" \
        "$helper_root/peritus-linux-sandbox-helper" "$share_root/peritus.service" \
        "$share_root/Uninstall-Peritus.sh"; do
        [ -f "$path" ] && [ ! -L "$path" ] || fail "existing package layout is partial or aliased"
    done
    legacy_digest=$(
        for path in "$bin_root/peritusd" "$bin_root/peritus" "$bin_root/peritus-tui" \
            "$helper_root/peritus-linux-sandbox-helper" "$share_root/peritus.service" \
            "$share_root/Uninstall-Peritus.sh"; do
            hash_file "$path"
        done | sha256sum | { read -r digest ignored; printf '%s\n' "$digest"; }
    )
    legacy="legacy-$legacy_digest"
    destination="$generation_root/$legacy"
    if [ ! -d "$destination" ]; then
        temporary="$generation_root/.stage-$legacy-$$"
        install -d -m 700 "$temporary/bin" "$temporary/libexec" "$temporary/share/peritus"
        cp -p "$bin_root/peritusd" "$bin_root/peritus" "$bin_root/peritus-tui" "$temporary/bin/"
        cp -p "$helper_root/peritus-linux-sandbox-helper" "$temporary/libexec/"
        cp -p "$share_root/peritus.service" "$temporary/share/peritus/peritus.service"
        cp -p "$share_root/Uninstall-Peritus.sh" "$temporary/Uninstall-Peritus.sh"
        printf '%s\n' "$legacy" > "$temporary/.source-sha256"
        sync
        mv "$temporary" "$destination"
        sync
    fi
    record "$migration" "version=1" "legacy=$legacy" "phase=staged"
}

recover_migration() {
    [ -f "$migration" ] || return
    legacy=$(field legacy "$migration")
    is_hex64 "${legacy#legacy-}" || fail "legacy generation identity is malformed"
    verify_retained_generation "$legacy" || fail "legacy recovery generation is incomplete"
    if [ ! -L "$current" ] || [ "$(readlink "$current")" != "generations/$legacy" ]; then
        atomic_current "$legacy" none
    fi
    record "$migration" "version=1" "legacy=$legacy" "phase=current"
    ensure_stable_links
    verify_stable_links || fail "stable command links could not be migrated"
    record "$receipt_root/migration-$legacy.receipt" \
        "version=1" "legacy=$legacy" "phase=verified"
    rm -f -- "$migration"
    sync
}

if [ -f "$migration" ]; then
    recover_migration
elif [ ! -L "$current" ]; then
    if [ -e "$bin_root/peritusd" ] || [ -e "$bin_root/peritus" ]; then
        stage_legacy_generation
        recover_migration
    fi
fi

write_active() {
    phase=$1
    target=$2
    previous=$3
    configuration=$4
    record "$active" "version=1" "target=$target" "previous=$previous" "phase=$phase" \
        "configuration=$configuration" "handoff=handoff-$target.receipt"
}

handoff() {
    target=$1
    configuration=$2
    owner=$3
    receipt="$transaction_root/handoff-$target.receipt"
    if [ "$owner" = none ]; then
        record "$receipt" "peritus-package-handoff-v1" "status=not-installed"
    elif [ "$configuration" != none ]; then
        installed_daemon="$generation_root/$owner/bin/peritusd"
        [ -x "$installed_daemon" ] || fail "installed daemon needed for exact handoff is unavailable"
        temporary="$receipt.new.$$"
        if "$installed_daemon" package-handoff --config "$configuration" > "$temporary"; then
            :
        else
            installed_status=$?
            if [ "$installed_status" -ne 2 ]; then
                mv -f "$temporary" "$receipt.failed"
                fail "the installed daemon rejected the exact durable handoff"
            fi
            if ! "$bundle/bin/peritusd" package-handoff-legacy --config "$configuration" > "$temporary"; then
                mv -f "$temporary" "$receipt.failed"
                fail "the installed daemon could not complete an exact durable handoff; stop the older client cleanly and retry"
            fi
        fi
        if [ "$(sed -n '1p' "$temporary")" != peritus-package-handoff-v1 ]; then
            mv -f "$temporary" "$receipt.failed"
            fail "the installed daemon returned a malformed handoff receipt"
        fi
        status=$(sed -n 's/^status=//p' "$temporary")
        case "$status" in
        clean) mv -f "$temporary" "$receipt" ;;
        already-stopped)
            if [ -f "$receipt" ]; then rm -f -- "$temporary"; else mv -f "$temporary" "$receipt"; fi
            ;;
        *)
            mv -f "$temporary" "$receipt.failed"
            fail "the installed daemon returned an invalid handoff receipt"
        esac
    else
        fail "an existing installation has no discoverable daemon configuration; set PERITUS_DAEMON_CONFIG and retry"
    fi
    sync
}

rollback_active() {
    target=$1
    previous=$2
    configuration=$3
    if [ "$previous" = none ]; then
        if [ "$configuration" != none ]; then
            handoff "$target" "$configuration" "$target"
        fi
        if [ -L "$current" ] && [ "$(readlink "$current")" = "generations/$target" ]; then
            if [ "$configuration" != none ]; then
                "$bundle/bin/peritusd" package-remove --store "$install_root" \
                    --current "$current" --expected "$generation_root/$target" \
                    --config "$configuration" || fail "unverified generation could not be removed"
            else
                "$bundle/bin/peritusd" package-remove --store "$install_root" \
                    --current "$current" --expected "$generation_root/$target" ||
                    fail "unverified generation could not be removed"
            fi
        fi
        remove_stable_links
        [ ! -e "$current" ] || fail "failed rollback retained an unverified current generation"
    else
        handoff "$target" "$configuration" "$target"
        verify_retained_generation "$previous" || fail "rollback generation $previous is incomplete"
        atomic_current "$previous" "$configuration"
        [ "$(readlink "$current")" = "generations/$previous" ] ||
            fail "rollback generation was not adopted"
        ensure_stable_links
        verify_stable_links || fail "rollback stable links are incomplete"
    fi
    record "$receipt_root/$target.rollback" \
        "version=1" "target=$target" "previous=$previous" "phase=rolled-back" \
        "configuration=$configuration"
    rm -f -- "$active"
    sync
}

finish_active() {
    target=$(field target "$active")
    previous=$(field previous "$active")
    phase=$(field phase "$active")
    configuration=$(field configuration "$active")
    is_hex64 "$target" || fail "target generation identity is malformed"
    case "$previous" in
        none) ;;
        legacy-*) is_hex64 "${previous#legacy-}" || fail "rollback generation identity is malformed" ;;
        *) is_hex64 "$previous" || fail "rollback generation identity is malformed" ;;
    esac
    validate_transaction_configuration "$configuration"
    if [ "$previous" != none ] && [ "$configuration" = none ]; then
        fail "an existing generation has no daemon configuration for the publication guard"
    fi
    if [ "$phase" = rollback ]; then
        rollback_active "$target" "$previous" "$configuration"
        return
    fi
    verify_retained_generation "$target" || fail "staged generation $target is incomplete"
    if [ "$phase" = staged ]; then
        handoff "$target" "$configuration" "$previous"
        write_active handoff "$target" "$previous" "$configuration"
        phase=handoff
    fi
    if [ "$phase" = handoff ]; then
        handoff "$target" "$configuration" "$previous"
        atomic_current "$target" "$configuration"
        write_active adopted "$target" "$previous" "$configuration"
        phase=adopted
    fi
    [ "$phase" = adopted ] || fail "durable install receipt has an unknown phase"
    ensure_stable_links
    if [ "$(readlink "$current")" != "generations/$target" ] || ! verify_stable_links; then
        write_active rollback "$target" "$previous" "$configuration"
        rollback_active "$target" "$previous" "$configuration"
        fail "new generation verification failed; the prior generation was restored"
    fi
    record "$receipt_root/$target.receipt" \
        "version=1" "target=$target" "previous=$previous" "phase=verified" \
        "configuration=$configuration" "handoff=handoff-$target.receipt"
    cp -p "$transaction_root/handoff-$target.receipt" "$receipt_root/handoff-$target.receipt"
    sync
    rm -f -- "$active" "$transaction_root/handoff-$target.receipt"
    sync
}

if [ -f "$active" ]; then
    finish_active
fi
stage_generation "$generation"
if [ -L "$current" ] && [ "$(readlink "$current")" = "generations/$generation" ]; then
    ensure_stable_links
    verify_generation "$generation" || fail "installed generation does not match the verified package"
else
    previous=none
    if [ -L "$current" ]; then
        current_target=$(readlink "$current")
        case "$current_target" in generations/*) previous=${current_target#generations/} ;;
            *) fail "current generation link is malformed" ;;
        esac
    fi
    configuration=$(configuration_for_transaction)
    if [ "$previous" != none ] && [ "$configuration" = none ]; then
        fail "an existing generation has no daemon configuration for the publication guard"
    fi
    write_active staged "$generation" "$previous" "$configuration"
    finish_active
fi

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
