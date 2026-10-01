# Architecture Overview

Crabtainer follows a **daemonless model** inspired by Podman.

## Running a container

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant CLI as crabtainer CLI
    participant Init as crabtainer_init (PID 1)
    participant Kernel as Linux Kernel (Cgroups/NS)

    User->>CLI: crabtainer run --name app my-image
    CLI->>Kernel: Create Namespaces & Cgroups v2
    CLI->>Init: Spawn crabtainer_init in target NS
    Init->>Kernel: Mount OverlayFS rootfs
    Init->>Init: Exec container command
    Init-->>CLI: Process exit code / status
```

## Key Architectural Concepts

1. **No Background Daemon:** Eliminates single point of failure and continuous background resource overhead.
2. **`crabtainer_init` (PID 1):** A dedicated, minimal Rust binary injected inside the container environment to handle signal forwarding, process reaping, and clean exit statuses.
3. **Storage & Isolation:** Uses Linux OverlayFS for layer composition and `cgroups v2` (`cgr
