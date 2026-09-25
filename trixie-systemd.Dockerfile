# This Dockerfile creates a Debian Trixie image with systemd configured to run in unprivileged containers.
# It is designed to be used for testing systemd-dependent services like nginx-hibernator.

FROM debian:trixie

LABEL maintainer="nginx-hibernator maintainers"
LABEL description="Debian Trixie with systemd for unprivileged containers"

# Inform systemd that it is running inside a container
ENV container=docker
ENV LC_ALL=C.UTF-8
ENV DEBIAN_FRONTEND=noninteractive

# Install systemd plus the Nginx and Python runtime used by scripts/run-container.sh.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    systemd \
    systemd-sysv \
    dbus \
    dbus-user-session \
    iproute2 \
    nginx \
    python3 \
    procps \
    curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Mask units that are unnecessary or problematic in a containerized environment.
# This prevents systemd from trying to access hardware or kernel features it doesn't have permissions for.
RUN systemctl mask \
    systemd-udevd.service \
    systemd-udev-trigger.service \
    systemd-modules-load.service \
    systemd-timesyncd.service \
    systemd-journal-flush.service \
    systemd-remount-fs.service \
    sys-kernel-debug.mount \
    sys-kernel-config.mount \
    dev-mqueue.mount \
    dev-hugepages.mount \
    display-manager.service \
    graphical.target \
    getty.target \
    getty@tty1.service

# Configure systemd behavior for containers
# Shorten timeouts to make testing faster and more responsive.
RUN mkdir -p /etc/systemd/system.conf.d && \
    printf "[Manager]\nDefaultTimeoutStartSec=15s\nDefaultTimeoutStopSec=15s\n" > /etc/systemd/system.conf.d/container.conf

# Ensure dbus is properly initialized
RUN mkdir -p /var/run/dbus && \
    chown messagebus:messagebus /var/run/dbus

# Create volumes for directories that systemd expects to be writeable and often tmpfs.
# At runtime, it is recommended to mount these as tmpfs for better performance and isolation.
VOLUME ["/sys/fs/cgroup", "/run", "/run/lock", "/tmp"]

# Systemd handles SIGRTMIN+3 as a request for a graceful shutdown.
STOPSIGNAL SIGRTMIN+3

# Start systemd as the init process (PID 1).
# We use multi-user.target as the default state.
CMD ["/lib/systemd/systemd", "--log-level=info", "--unit=multi-user.target"]

# --- Usage Instructions ---
#
# Build the image:
#   docker build -t trixie-systemd -f trixie-systemd.Dockerfile .
#
# Run the container unprivileged (requires cgroup v2 host):
#   docker run -d \
#     --name trixie-systemd \
#     --tmpfs /run --tmpfs /run/lock \
#     -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
#     --cgroupns=host \
#     trixie-systemd
#
# Enter the container:
#   docker exec -it trixie-systemd bash
