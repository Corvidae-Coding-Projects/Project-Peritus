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
fail() { printf '%s\n' "Peritus install failed: $*" >&2; exit 1; }
case "${PERITUS_INSTALL_DEPS:-1}" in 0|1) ;; *) fail "PERITUS_INSTALL_DEPS must be 0 or 1" ;; esac
if ! git --version >/dev/null 2>&1; then
    [ "${PERITUS_INSTALL_DEPS:-1}" = 1 ] || fail "install Git, then retry"
    if command -v brew >/dev/null 2>&1; then
        brew install git </dev/null
        PATH="$(brew --prefix)/bin:$PATH"
        export PATH
    else
        printf '%s\n' "Approve Apple's Command Line Tools installation window. Peritus will wait up to 20 minutes."
        xcode-select --install || fail "could not start Apple's Git installation"
        attempts=0
        until xcode-select -p >/dev/null 2>&1; do
            [ "$attempts" -lt 240 ] || fail "Git installation did not finish; complete it and run this installer again"
            sleep 5
            attempts=$((attempts + 1))
        done
    fi
fi
git --version >/dev/null 2>&1 || fail "Git installation did not provide a working git command"
"$bundle/bin/peritus" --version >/dev/null || fail "the package cannot run on this macOS version"
umask 077
install -d -m 700 "$bin_root" "$helper_root" "$share_root" "$command_root"
publish() { source_file=$1; target_file=$2; target_mode=$3; temporary="$target_file.new.$$"; install -m "$target_mode" "$source_file" "$temporary"; mv -f "$temporary" "$target_file"; }
publish "$bundle/bin/peritusd" "$bin_root/peritusd" 755
publish "$bundle/bin/peritus" "$bin_root/peritus" 755
publish "$bundle/bin/peritus-tui" "$bin_root/peritus-tui" 755
publish "$bundle/libexec/peritus-macos-sandbox-helper" "$helper_root/peritus-macos-sandbox-helper" 755
publish "$bundle/share/peritus/com.corvidae.peritus.plist.in" "$share_root/com.corvidae.peritus.plist.in" 600
publish "$bundle/Uninstall-Peritus.sh" "$share_root/Uninstall-Peritus.sh" 700
ln -sfn "$bin_root/peritus" "$command_root/peritus"
case "${SHELL:-/bin/zsh}" in
    */zsh) profile="$peritus_user_home/.zshrc" ;;
    */bash) profile="$peritus_user_home/.bash_profile" ;;
    */fish) profile="${XDG_CONFIG_HOME:-$peritus_user_home/.config}/fish/conf.d/peritus.fish" ;;
    *) profile="$peritus_user_home/.profile" ;;
esac
path_line="export PATH=\"\$HOME/.local/bin:\$PATH\""
case "${SHELL:-/bin/zsh}" in */fish) path_line="fish_add_path --path \"\$HOME/.local/bin\"" ;; esac
if [ -L "$profile" ]; then
    printf '%s\n' "Shell configuration is a symbolic link; add this line yourself: $path_line" >&2
else
    mkdir -p "$(dirname "$profile")"
    if ! grep -Fqx "$path_line" "$profile" 2>/dev/null; then
        printf '\n%s\n%s\n' '# Peritus user commands' "$path_line" >> "$profile"
    fi
fi
printf '%s\n' "Peritus installed. Open a new terminal and run: peritus"
printf '%s\n' "For this terminal, run: $command_root/peritus"
