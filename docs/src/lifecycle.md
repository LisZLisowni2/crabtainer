# Managing Containers

Containers are created from built layouts with `crabtainer run` and managed
with the containers-focused commands. This page covers their life cycle and
how to drive it.

## Container states

A container's status is persisted in `containers/<id>/config.json` and can
be one of:

| Status | Meaning |
| --- | --- |
| `Active` | The container process is currently running. |
| `Exited` | The container finished with exit code `0`. |
| `Error` | The container finished with a non-zero (and non-signal) exit code. |
| `Stopped` | The container was terminated by a signal (exit code `143` / `SIGTERM`, e.g. via `crabtainer stop`). |

Before most commands run, a *refresh* pass reaps finished processes and
rewrites these states. It also detects containers left over from a previous
host boot by comparing their stored `boot_id` with the current one — such
containers are marked `Exited` rather than left as phantom `Active` entries.

## Run modes

```bash
# Foreground — output goes straight to your terminal
sudo crabtainer run myapp-v1

# Detached — background, output to containers/<id>/container.log
sudo crabtainer run -d --name app myapp-v1

# One-shot — remove the container directory after it exits
sudo crabtainer run --rm myapp-v1
```

* A foreground run blocks until the container exits and prints its output.
  `Ctrl+C` forwards `SIGINT` to the container.
* A detached run prints the container ID and returns. Logs are available with
  `crabtainer logs <name>`.
* `--rm` cleans up the container's overlay layers and config as soon as the
  process exits.
* `-n/--name` gives the container a stable, unique name to target with later
  commands.

For all flags see [CLI Reference — run](cli-reference.md#run--create-and-start-a-container).

## Lifecycle operations

| Command | Effect |
| --- | --- |
| `crabtainer ps` | List all containers with their current status. |
| `crabtainer stop <name>` | Send `SIGTERM` (only `Active` containers). |
| `crabtainer start <name>` | Start a stopped container again with its original options (detached containers re-spawn detached). |
| `crabtainer restart <name>` | Stop, then start, an `Active` container. |
| `crabtainer exec [-i] [-t] <name> <cmd>` | Run a command inside a running container — `-t` gives a PTY, `-i` forwards stdin (`-it` for a shell). |
| `crabtainer inspect <name>` | Print the persisted `config.json` (runtime state). |
| `crabtainer logs <name>` | View `container.log` in `less` (detached containers). |
| `crabtainer rm <name>` / `crabtainer rm .` | Remove one stopped/exited container, or all of them (with confirmation). |

## Restart policies

Set with `-r/--restart` when running (or baked into the layout):

| Policy | Restart behavior |
| --- | --- |
| `never` (default) | Never restarted. |
| `on-failure` | Restarted only if the container exited with an error. |
| `unless-stopped` | Restarted unless it was explicitly stopped. |
| `always` | Always restarted. |

Detached containers with a restart policy are re-spawned after a crash or
host reboot by the autostart pass — run `crabtainer system autostart`
manually, or let it run from systemd at boot (installed by
`crabtainer system init`, which creates a `crabtainer-autostart.service`
pointing at the current binary).

## Signals & exit behavior

* `crabtainer stop`, host shutdown, or `Ctrl+C` in a foreground run delivers
  `SIGTERM`/`SIGINT`, which `crabtainer_init` (the container's PID 1)
  forwards to the containerized process. Zombie children are reaped and the
  original exit code is propagated back to the CLI.
* `SIGTERM` (exit `143`) maps to the `Stopped` state — that's what lets
  `unless-stopped` distinguish "stopped on purpose" from "crashed".

## Cleanup

Containers accumulate runtime data in `containers/<id>/`. There is no
automatic GC; prune manually:

* `crabtainer rm <name>` — remove an individual stopped/exited container.
* `crabtainer rm .` — remove all stopped/exited containers (confirmation
  prompt).
* Layouts (`crabtainer layout rm`) and images (`crabtainer image rm`) are
  shared, so they're only removed on explicit request — and a layout cannot
  be removed while a container still references it.

> [!NOTE]
> `crabtainer system prune` and `crabtainer volume prune` exist in the CLI as
> no-op stubs and are **not implemented yet**.