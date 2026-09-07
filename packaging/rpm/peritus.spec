%{!?peritus_version:%{error:Pass --define 'peritus_version X.Y.Z'}}
%{!?peritus_packager:%{error:Pass --define 'peritus_packager Name <email>'}}
%{!?peritus_changelog_date:%{error:Pass --define 'peritus_changelog_date Sun Sep 06 2026'}}
Name:           peritus
Version:        %{peritus_version}
Release:        1%{?dist}
Summary:        Local-first evidence-driven coding agent
# Upstream plus the selected compatible licenses of locked Linux normal/build
# dependencies. Full alternative expressions and upstream texts are retained in
# THIRD-PARTY-NOTICES.txt; review this field whenever Cargo.lock changes.
License:        MIT AND Apache-2.0 AND BSD-3-Clause AND ISC AND Unicode-3.0 AND Zlib
URL:            https://github.com/Corvidae-Coding-Projects/Project-Peritus
Source0:        peritus-%{version}.tar.gz
Packager:       %{peritus_packager}
ExclusiveArch:  x86_64 aarch64
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  make
BuildRequires:  rustup
BuildRequires:  pkgconf-pkg-config
BuildRequires:  python3
Requires:       ca-certificates
Requires:       git-core
Requires:       bubblewrap
Requires:       dbus-daemon
Recommends:     gnome-keyring

%description
Peritus provides a terminal interface, command-line client, supervised per-user
daemon, and Linux sandbox helper. It runs as the invoking user and does not
install or start a system daemon. Updates belong to the system package manager.

%prep
%autosetup

%build
export CARGO_HOME="$PWD/.package-cargo"
export CARGO_NET_OFFLINE=true
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export CARGO_PROFILE_RELEASE_DEBUG=2
export CARGO_PROFILE_RELEASE_STRIP=none
export RUSTFLAGS="-C force-frame-pointers=yes -C link-arg=-Wl,-z,relro,-z,now"
cargo --config .cargo/vendor.toml build --release --frozen --bins \
    -p peritus-cli -p peritus-daemon -p peritus-tui -p peritus-sandbox-linux \
    --features peritus-cli/system-package

%check
export CARGO_HOME="$PWD/.package-cargo"
export CARGO_NET_OFFLINE=true
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export CARGO_PROFILE_RELEASE_DEBUG=2
export CARGO_PROFILE_RELEASE_STRIP=none
export RUSTFLAGS="-C force-frame-pointers=yes -C link-arg=-Wl,-z,relro,-z,now"
cargo --config .cargo/vendor.toml test --frozen -p peritus-launcher \
    --features system-package update::tests
test "$(target/release/peritus --version)" = 'peritus %{version}'

%install
install -d %{buildroot}%{_prefix}/lib/peritus %{buildroot}%{_bindir}
install -m 0755 target/release/peritus target/release/peritusd \
    target/release/peritus-tui target/release/peritus-linux-sandbox-helper \
    %{buildroot}%{_prefix}/lib/peritus/
ln -s ../lib/peritus/peritus %{buildroot}%{_bindir}/peritus
install -Dm 0644 packaging/man/peritus.1 %{buildroot}%{_mandir}/man1/peritus.1

%files
%license LICENSE THIRD-PARTY-NOTICES.txt
%doc README.md
%{_bindir}/peritus
%{_prefix}/lib/peritus/
%{_mandir}/man1/peritus.1*

%changelog
* %{peritus_changelog_date} %{peritus_packager} - %{version}-1
- Build the upstream release with locked, vendored dependencies.
