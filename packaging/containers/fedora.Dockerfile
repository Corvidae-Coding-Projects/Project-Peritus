FROM registry.fedoraproject.org/fedora:44@sha256:125c2106ffaa3b2035c5bb480355c6bee73f723bfab47012bb29269d97ec61e4
ARG RUST_VERSION=1.97.1
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
ENV PATH="/opt/cargo/bin:${PATH}"
RUN dnf install -y --setopt=install_weak_deps=False gcc gcc-c++ make rpm-build rpm-sign rustup \
    pkgconf-pkg-config python3 ca-certificates gnupg2 \
    git-core bubblewrap dbus-daemon gnome-keyring \
    && dnf clean all
RUN rustup-init -y --no-modify-path --profile minimal --component clippy,rustfmt --default-toolchain ${RUST_VERSION} \
    && chmod -R a+rX /opt/rustup /opt/cargo
WORKDIR /build
