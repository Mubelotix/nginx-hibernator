FROM debian:trixie AS builder

ARG DEB_PACKAGE_NAME=nginx-hibernator-module
ARG DEB_PACKAGE_VERSION=0.1.0
ARG DEB_PACKAGE_RELEASE=1
ARG DEB_MAINTAINER="nginx-hibernator maintainers <maintainers@example.com>"
ARG DEB_DESCRIPTION="NGINX dynamic module for automatic hibernation and wake-up of upstream services"

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Enable deb-src entries so apt can download nginx source and build dependencies.
RUN sed -i 's/^Types: deb$/Types: deb deb-src/' /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        build-essential \
        dpkg-dev \
        pkg-config \
        clang \
        libclang-dev \
        libdbus-1-dev \
        rustup \
        nginx \
    && apt-get build-dep -y nginx \
    && rustup toolchain install stable --profile minimal \
    && rustup default stable \
    && mkdir -p /tmp/nginx-src \
    && cd /tmp/nginx-src \
    && apt-get source nginx \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work
COPY . .

# Build nginx from Debian source using the same configure arguments as Debian nginx
RUN mkdir -p /tmp/nginx-src \
    && cd /tmp/nginx-src \
    && NGINX_SRC_DIR="$(find . -maxdepth 1 -mindepth 1 -type d -name 'nginx-*' | head -n 1)" \
    && test -n "$NGINX_SRC_DIR" \
    && cd "$NGINX_SRC_DIR" \
    && NGINX_CONF_ARGS="$(nginx -V 2>&1 | sed -n 's/^.*arguments: //p')" \
    && eval "./configure ${NGINX_CONF_ARGS} --with-compat" \
    && make -j"$(nproc)"

# Build the module against the nginx source tree for ABI compatibility
RUN export NGINX_SOURCE_DIR="$(find /tmp/nginx-src -maxdepth 1 -mindepth 1 -type d -name 'nginx-*' | head -n 1)" \
    && export NGINX_BUILD_DIR="${NGINX_SOURCE_DIR}/objs" \
    && cd /work \
    && cargo build --release --locked

# Create Debian package
RUN mkdir -p /tmp/pkg/DEBIAN \
             /tmp/pkg/usr/lib/nginx/modules \
             /tmp/pkg/etc/nginx/modules-available \
             /tmp/pkg/etc/nginx/modules-enabled \
             /artifacts \
    && install -m 0644 /work/target/release/libhibernator.so \
        /tmp/pkg/usr/lib/nginx/modules/libhibernator.so \
    && printf '%s\n' 'load_module /usr/lib/nginx/modules/libhibernator.so;' \
        > /tmp/pkg/etc/nginx/modules-available/50-mod-hibernator.conf \
    && ln -s ../modules-available/50-mod-hibernator.conf \
        /tmp/pkg/etc/nginx/modules-enabled/50-mod-hibernator.conf

# Build the deb package
RUN ARCH="$(dpkg --print-architecture)" \
    && INSTALLED_SIZE="$(du -sk /tmp/pkg/usr /tmp/pkg/etc | awk '{sum += $1} END {print sum}')" \
    && printf 'Package: %s\nVersion: %s-%s\nSection: web\nPriority: optional\nArchitecture: %s\nDepends: nginx, libdbus-1-3\nMaintainer: %s\nInstalled-Size: %s\nDescription: %s\n' \
        "${DEB_PACKAGE_NAME}" "${DEB_PACKAGE_VERSION}" "${DEB_PACKAGE_RELEASE}" "${ARCH}" "${DEB_MAINTAINER}" "${INSTALLED_SIZE}" "${DEB_DESCRIPTION}" \
        > /tmp/pkg/DEBIAN/control \
    && chmod 0644 /tmp/pkg/DEBIAN/control \
    && dpkg-deb --build --root-owner-group /tmp/pkg \
        "/artifacts/${DEB_PACKAGE_NAME}_${DEB_PACKAGE_VERSION}-${DEB_PACKAGE_RELEASE}_${ARCH}.deb"

FROM scratch AS artifact
    COPY --from=builder /artifacts /artifacts
