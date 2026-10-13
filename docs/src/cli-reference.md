# CLI Reference

All commands must be run as **root** (or via `sudo`); the binary refuses to
start otherwise. Run `sudo crabtainer --help` for usage summaries at any time.

Global syntax:

```text
crabtainer <command> [options]
```

---

## `run` — Create and start a container

```text
crabtainer run [OPTIONS] <LAYOUT> [-c <COMMAND>] [ARGS]...
```

Runs a container from a previously built layout.

| Option | Description |
| --- | --- |
| `<LAYOUT>` | Layout tag to run (positional). |
| `-d, --detach` | Run in the background and print the container ID. Output is redirected to `containers/<id>/container.log`. |
| `--rm` | Remove the container directory after it exits. |
| `-n, --name <NAME>` | Assign a human-friendly name (must be unique). |
| `-C, --cpu-limit <CORES>` | Maximum CPU limit in cores, e.g. `1.5`. |
| `-M, --memory-limit <MB>` | Maximum memory limit in MB (numeric value, e.g. `2048`). |
| `-p, --port <HOST_CONTAINER>` | Forward a port (repeatable), e.g. `-p 8000:80`. See [Networking](networking.md). |
| `-e, --env <KEY=VALUE>` | Set an environment variable (repeatable). |
| `-v, --volume <SRC:DST>` | Mount a named volume or host path (repeatable). See [Storage and Volumes](storage-and-volumes.md). |
| `-r, --restart <POLICY>` | Restart policy: `never` (default), `onfailure`, `unless-stopped`, `always`. See [Managing Containers](lifecycle.md). |
| `-c, --command <CMD>` | Override the container's command. Trailing arguments are passed to it. |

Examples:

```bash
# Foreground run with the layout's default command
sudo crabtainer run myapp-v1

# Detached, named, with limits, port forwarding, env and a volume
sudo crabtainer run -d --name production-app \
  -C 2.0 -M 1024 -p 8080:80 \
  -e MODE=release -v data:/app/data \
  myapp-v1

# Run a custom command
sudo crabtainer run --name debug myapp-v1 -c /bin/sh echo hello
```

---

## `build` — Build a layout from a Crabtainerfile

```text
crabtainer build [-f <FILE>] -t <TAG>
```

Builds a layout (rootfs + OCI config) from a `Crabtainerfile`.

| Option | Description |
| --- | --- |
| `-f, --file <FILE>` | Path to the Crabtainerfile. Defaults to `./Crabtainerfile`. |
| `-t, --tag <TAG>` | Tag assigned to the built layout. |

Fails if a layout with that tag already exists.

```bash
sudo crabtainer build -f Crabtainerfile -t myapp-v1
```

---

## `ps` — List containers

```text
crabtainer ps
```

Refreshes container states (detects crashed/rebooted containers) and prints a
table of `ID`, `NAME`, `LAYOUT`, and `STATUS`.

---

## `start` — Start a stopped container

```text
crabtainer start <NAME>
```

Starts a stopped container again with its original options (layout, args,
limits, ports, volumes). Detached containers are re-spawned detached.

---

## `restart` — Restart an active container

```text
crabtainer restart <NAME>
```

Stops and then starts an **active** container. Non-active containers are
rejected.

---

## `stop` — Stop an active container

```text
crabtainer stop <NAME>
```

Sends `SIGTERM` to the container's root process. The container's init
(`crabtainer_init`) forwards the signal to the containerized command.
Nothing happens if the container is not `Active`.

---

## `rm` — Remove containers

```text
crabtainer rm <NAME|".">
```

Removes the runtime directory of a stopped/exited container. Active
containers must be stopped first.

The special value `.` removes **all** stopped and exited containers after a
`y/n` confirmation.

---

## `exec` — Execute a command inside a container

```text
crabtainer exec [-i] [-t] <NAME> <CMD> [ARGS]...
```

Joins the container's network, PID, and mount namespaces and runs a command
inside it.

| Option | Description |
| --- | --- |
| `-i, --interactive` | Forward host stdin to the process. |
| `-t, --tty` | Allocate a PTY and run a full terminal session. |

```bash
sudo crabtainer exec -it myapp /bin/sh
sudo crabtainer exec myapp env
```

---

## `inspect` — Show container details

```text
crabtainer inspect <NAME>
```

Prints the container's persisted `config.json` (runtime state).

---

## `logs` — Show container logs

```text
crabtainer logs <NAME>
```

Opens the container's `container.log` in `less`. Only detached containers
(`run -d`) produce a log file.

---

## `image` — Manage images

```text
crabtainer image <ACTION>
```

| Action | Description |
| --- | --- |
| `image ps` | List downloaded images with their sizes. |
| `image pull <IMAGE> <ALIAS> [-o]` | Download an image from a registry (e.g. Docker Hub) under `<ALIAS>`. `-o, --overriding` replaces an existing image with the same alias. |
| `image rm <NAME\|".">` | Remove an image; `.` removes all images after confirmation. |
| `image inspect <NAME>` | Print the image config in JSON. |

```bash
sudo crabtainer image pull ubuntu:24.04 ubuntu
sudo crabtainer image ps
```

---

## `layout` — Manage layouts

```text
crabtainer layout <ACTION>
```

| Action | Description |
| --- | --- |
| `layout ps` | List built layouts with their sizes. |
| `layout rm <TAG\|".">` | Remove a layout. Refuses when a container still uses it; `.` removes all unused layouts after confirmation. |
| `layout inspect <TAG>` | Print the layout's OCI spec (`config.json`) in JSON. |

---

## `volume` — Manage volumes

```text
crabtainer volume <ACTION>
```

| Action | Description |
| --- | --- |
| `volume create <NAME>` | Create a new named volume. |
| `volume ls` | List volumes and their sizes. |
| `volume remove <NAME>` | Delete a volume directory. |
| `volume prune` | **Not implemented** — currently a no-op stub. |

---

## `system` — System-level configuration

```text
crabtainer system <ACTION>
```

| Action | Description |
| --- | --- |
| `system init` | Write the system configuration: enables `net.ipv4.conf.all.route_localnet` via `/etc/sysctl.d/99-crabtainer.conf` and, on systemd hosts, installs the `crabtainer-autostart.service` unit. |
| `system autostart` | Start all detached containers whose restart policy says they should be running (runs automatically on boot via the systemd unit). |
| `system prune` | **Not implemented** — currently a no-op stub. |