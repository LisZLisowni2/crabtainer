# Quick Start Guide

This guide walks you through building your first layout and running a
container.

## Prerequisites

- Linux (Kernel 5.x or newer recommended)
- `cgroups v2` enabled
- OverlayFS support and Linux namespaces
- `iptables` (for container networking)
- Root privileges (`sudo`) — the binary refuses to start otherwise
- A Rust toolchain if building from source (`cargo build --release`)

> [!WARNING]
> Alpine-based systems aren't currently supported.

## Step 1: Build Crabtainer

```bash
cargo build --release
```

This produces `target/release/crabtainer` and — next to it — the
`crabtainer_init` binary that is injected into every container as PID 1.

## Step 2: Initialize System Configuration (optional)

Set up Crabtainer's network sysctl settings and, on systemd hosts, install the
autostart service:

```bash
sudo ./target/release/crabtainer system init
```

This step is only needed once per host (and is only required for
networking/autostart — it is not needed to build images or run foreground
containers on an already-configured host).

## Step 3: Create a Crabtainerfile

Create a file named `Crabtainerfile` in your working directory:

```Dockerfile
DOWNLOAD debian:trixie AS debian
FROM debian
WORKDIR /app
COPY ./app.sh ~
CPU_LIMIT 1.0
MEMORY_LIMIT 512m
CMD /app/app.sh
```

## Step 4: Build the Layout

```bash
sudo ./target/release/crabtainer build --tag myapp-v1
```

The layout — a ready-to-run rootfs plus OCI config — is stored under
`/var/lib/crabtainer/layouts/myapp-v1`.

## Step 5: Run the Container

```bash
# Run interactively (foreground)
sudo ./target/release/crabtainer run myapp-v1

# Run in background with resource limits and port forwarding
sudo ./target/release/crabtainer run -d --name production-app \
  -C 2.0 -M 1024 -p 8080:80 myapp-v1

# Override the default command
sudo ./target/release/crabtainer run -c /bin/sh myapp-v1
```

> [!NOTE]
> `-M` takes a **number of megabytes** (e.g. `-M 1024`, not `1024m`). The
> `m`/`g` suffix style is only used by the `MEMORY_LIMIT` instruction inside
> a Crabtainerfile.

## Step 6: Manage the Container

```bash
sudo ./target/release/crabtainer ps                # list containers
sudo ./target/release/crabtainer logs production-app   # detached container logs
sudo ./target/release/crabtainer exec -it production-app /bin/sh
sudo ./target/release/crabtainer stop production-app
sudo ./target/release/crabtainer rm production-app
```

That's it — a daemonless container with resource limits, built from scratch
on your machine. See the [Crabtainerfile Reference](crabtainerfile.md),
[CLI Reference](cli-reference.md), and [Managing Containers](lifecycle.md) for
the full picture.