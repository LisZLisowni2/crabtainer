# Introduction to Crabtainer

**Crabtainer** is a lightweight, daemonless container engine written from
scratch in Rust. It combines an image builder, a layout manager, and a
container runtime in a single root-only binary — no daemon, no client/server
split, no orchestration.

## Key Features

- **Daemonless:** No background service. Every container is managed directly
  by its CLI invocation; container state lives in plain files under
  `/var/lib/crabtainer` (or `$CRABTAINER_HOME`).
- **Custom Image Builder:** Build reproducible rootfs layouts using
  declarative `Crabtainerfile` specs, pulling base images from Docker Hub and
  other OCI registries.
- **Resource Limits:** Fine-grained CPU and memory isolation powered by Linux
  `cgroups v2`.
- **Complete Isolation:** Each container runs in its own UTS, PID, mount, and
  network namespaces with an OverlayFS rootfs, guarded by a minimal
  `crabtainer_init` as PID 1.
- **Container Networking:** A per-host `crabtainer0` bridge with IPAM,
  NAT, and port forwarding (`-p 8000:80`).
- **Persistence:** Named volumes and bind mounts survive container removal;
  restart policies plus boot-time autostart (systemd) keep detached workloads
  alive.
- **Fast & Safe:** Built with Rust's memory safety and zero-cost
  abstractions.

## Documentation Map

| Page | What it covers |
| --- | --- |
| [Quick Start](quickstart.md) | Get a container running in five minutes |
| [Crabtainerfile Reference](crabtainerfile.md) | Build-file syntax and `.crabtainerignore` |
| [CLI Reference](cli-reference.md) | Every command, flag, and example |
| [Managing Containers](lifecycle.md) | Run modes, restart policies, exec, cleanup |
| [Networking](networking.md) | Bridge, IPAM, port forwarding, troubleshooting |
| [Storage and Volumes](storage-and-volumes.md) | Directory layout, images/layouts, volumes |
| [Daemonless Architecture](architecture.md) | How the runtime works under the hood |
| [Troubleshooting](troubleshooting.md) | Common problems and their fixes |

## Requirements

- Linux with `cgroups v2`, OverlayFS, namespaces, and `iptables`
- Root privileges (`sudo`)
- systemd is optional — only needed for boot-time autostart

> [!WARNING]
> Alpine-based systems aren't currently supported.