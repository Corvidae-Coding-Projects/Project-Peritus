FROM docker.io/library/debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132
ARG RUST_VERSION=1.97.1
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
RUN apt-get update && apt-get install --no-install-recommends -y \
    build-essential debhelper devscripts dpkg-dev fakeroot lintian rustup \
    pkgconf python3 ca-certificates gnupg debsigs debsig-verify \
    git bubblewrap dbus-user-session gnome-keyring \
    && rm -rf /var/lib/apt/lists/*
RUN rustup toolchain install ${RUST_VERSION} --profile minimal --component clippy,rustfmt \
    && rustup default ${RUST_VERSION} \
    && mkdir -p /opt/cargo \
    && chmod -R a+rX /opt/rustup /opt/cargo
WORKDIR /build
