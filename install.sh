#!/bin/sh
set -eu

repository=${PERITUS_REPOSITORY:-Corvidae-Coding-Projects/Project-Peritus}
release_base=${PERITUS_RELEASE_BASE_URL:-https://github.com/$repository/releases/download}
# The release publisher replaces this token. A release bootstrap stays bound to its own tag.
release_tag='@PERITUS_RELEASE_TAG@'

fail() {
    printf '%s\n' "Peritus install failed: $*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

need curl
need tar
need sed
need tr

case "$(uname -s)" in
    Linux) platform=linux ;;
    Darwin) platform=macos ;;
    *) fail "this installer supports Linux and macOS; use install.ps1 on Windows" ;;
esac

case "$(uname -m)" in
    x86_64|amd64) architecture=x86_64 ;;
    arm64|aarch64) architecture=aarch64 ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
esac

version=${PERITUS_VERSION:-$release_tag}
case "$version" in @*) version= ;; esac
if [ -z "$version" ]; then
    latest=$(curl --fail --silent --show-error --location --output /dev/null \
        --proto '=https' --proto-redir '=https' \
        --write-out '%{url_effective}' "https://github.com/$repository/releases/latest") ||
        fail "could not resolve the latest GitHub release"
    version=${latest##*/}
fi
release_number=${version#v}
[ "$release_number" != "$version" ] || fail "release version is not a vMAJOR.MINOR.PATCH tag: $version"
case "$version" in *[!v0-9.]*) fail "release version is not a vMAJOR.MINOR.PATCH tag: $version" ;; esac
major=${release_number%%.*}
remainder=${release_number#*.}
[ "$remainder" != "$release_number" ] || fail "release version is not a vMAJOR.MINOR.PATCH tag: $version"
minor=${remainder%%.*}
patch=${remainder#*.}
[ "$patch" != "$remainder" ] || fail "release version is not a vMAJOR.MINOR.PATCH tag: $version"
case "$patch" in *.*) fail "release version is not a vMAJOR.MINOR.PATCH tag: $version" ;; esac
for component in "$major" "$minor" "$patch"; do
    [ -n "$component" ] || fail "release version is not a vMAJOR.MINOR.PATCH tag: $version"
    case "$component" in *[!0-9]*) fail "release version is not a vMAJOR.MINOR.PATCH tag: $version" ;; esac
done

asset="peritus-$platform-$architecture.tar.gz"
archive_url="$release_base/$version/$asset"
checksum_url="$archive_url.sha256"
temporary=$(mktemp -d "${TMPDIR:-/tmp}/peritus-install.XXXXXXXX") ||
    fail "could not create a temporary directory"
cleanup() { rm -rf -- "$temporary"; }
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

printf '%s\n' "Downloading Peritus $version for $platform/$architecture..."
curl --fail --silent --show-error --location --retry 3 \
    --proto '=https,file' --proto-redir '=https' --max-filesize 1073741824 \
    --output "$temporary/$asset" "$archive_url" || fail "could not download $archive_url"
curl --fail --silent --show-error --location --retry 3 \
    --proto '=https,file' --proto-redir '=https' --max-filesize 1024 \
    --output "$temporary/$asset.sha256" "$checksum_url" ||
    fail "could not download $checksum_url"

expected=$(tr -d ' \t\r\n' < "$temporary/$asset.sha256" | tr 'A-F' 'a-f')
case "$expected" in *[!0-9A-Fa-f]*) fail "release checksum is malformed" ;; esac
[ "${#expected}" -eq 64 ] || fail "release checksum is malformed"
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$temporary/$asset" | sed 's/[[:space:]].*$//')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$temporary/$asset" | sed 's/[[:space:]].*$//')
else
    fail "sha256sum or shasum is required"
fi
[ "$actual" = "$expected" ] || fail "release archive checksum did not match"

tar -tzf "$temporary/$asset" > "$temporary/entries" || fail "could not inspect the release archive"
while IFS= read -r entry; do
    case "$entry" in
        "peritus-$platform-$architecture"|"peritus-$platform-$architecture/"*) ;;
        *) fail "release archive contains a path outside the package" ;;
    esac
    case "/${entry%/}/" in *'/../'*|*'/./'*|*'//'*) fail "release archive contains an unsafe path" ;; esac
done < "$temporary/entries"
tar -tvzf "$temporary/$asset" > "$temporary/types" || fail "could not inspect archive entry types"
while IFS= read -r entry; do
    case "$entry" in -*|d*) ;; *) fail "release archive contains a link or special file" ;; esac
done < "$temporary/types"
tar -xzf "$temporary/$asset" -C "$temporary" || fail "could not extract the release archive"
bundle="$temporary/peritus-$platform-$architecture"
[ -d "$bundle" ] || fail "release archive did not contain $bundle"

case "$platform" in
    linux) installed="$HOME/.local/bin/peritus" ;;
    macos) installed="$HOME/Library/Application Support/Peritus/bin/peritus" ;;
esac
if [ -f "$installed" ]; then
    sh "$bundle/Upgrade-Peritus.sh" "$bundle" || fail "native upgrade failed and was rolled back"
else
    sh "$bundle/Install-Peritus.sh" "$bundle" || fail "native installation failed"
fi

observed=$("$installed" --version) || fail "the installed command does not run"
[ "$observed" = "peritus $release_number" ] || fail "the installed version does not match $version"
printf '%s\n' "Peritus $version is installed. Open a new terminal and run: peritus"
