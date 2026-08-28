# AGENTS.md

Daemonless Linux container engine (image builder + runtime) in Rust, edition 2024 (needs Rust 1.85+, repo built with 1.96). Cargo workspace, version 0.3.2.

## Commands

- `cargo build` / `cargo test` — workspace-wide; all unit tests are unprivileged-safe (tempdir + fake env), so `cargo test` works without root.
- `cargo fmt` / `cargo fmt --check` — run after edits; no rustfmt config, defaults apply. (Repo history contains a dedicated "Cargo fmt" commit.)
- `cargo clippy -- -D warnings` — required by CONTRIBUTING.md before PRs.
- CI (`.github/workflows/rust.yaml`) only runs `cargo build` + `cargo test --verbose` on PRs to `main`.

## Runtime requirements (all commands)

The binary refuses to start without root (`is_root` check in `src/bin/crabtainer.rs`). Every container run needs: Linux, cgroups v2, overlayfs, namespaces, iptables, sudo. systemd is optional (autostart service). Runtime behavior can't be smoke-tested from an unprivileged agent shell — rely on `cargo test` for verification and keep new tests root-free.

## Layout

Workspace with two members:

- `crates/crabtainer/` — main crate (lib + the only binary).
  - `src/bin/crabtainer.rs` — CLI + main. clap-derive; changing subcommands means updating the `Commands` enum and its arg-parsing tests in the same file. Subcommands: `run` (`-d/--detach`, `--rm`, `-n/--name`, `-C/--cpu-limit`, `-M/--memory-limit`, `-r/--restart` policy, `-c/--command` + trailing args), `build -f -t`, `ps`, `stop <name>`, `rm <name|".">`, `exec [-i] [-t] <name> <cmd>`, `images`, `layouts`, `system {prune|init-systemd|autostart}` (`prune` is a stub).
  - `src/engine/build/` — `crabtainerfile.rs` (parser/instruction model), `spec.rs` (OCI spec generation for layouts), `builder.rs` (orchestrator), `instructions/` (`download.rs` via oci-client from Docker Hub, `from.rs`, `copy.rs`, `run.rs`).
  - `src/engine/runtime/` — container lifecycle (`container.rs`: overlayfs mount, namespaces, pivot_root, detached fork w/ pid+log files), `exec.rs`, `network.rs` (rtnetlink veth, bridge `crabtainer0`, IPAM, iptables MASQUERADE), `cgroups.rs`, `options.rs`, `refresh.rs` (reap dead containers), `stop.rs`, `autostart.rs` (restarts detached containers per restart policy), `start.rs` (manual start of stopped containers; WIP, not wired into CLI yet).
  - `src/engine/support/` — `paths.rs` (`CrabtainerPaths`; `CRABTAINER_HOME` env var overrides default base dir `/var/lib/crabtainer`), `systemd.rs` (writes `/etc/systemd/system/crabtainer-autostart.service` pointing at the current exe), `test_utils.rs`.
- `crates/crabtainer_init/` — minimal PID 1 init (`crabtainer_init`). Forks the container command, forwards SIGINT/SIGTERM, reaps zombies, propagates exit codes. Must exist next to the `crabtainer` binary in the same target dir (both are built by the workspace); at runtime it's bind-mounted read-only into the container as `/dev/.crabtainer_init` and the containerized process is always exec'd through it (see `child_process` in `container.rs`).

## State

- Base dir subdirs: `images/`, `layouts/`, `containers/`. Initialized by `CrabtainerPaths::init_system_dirs()` (called from image download).
- One directory per container under `containers/<id>/` containing `config.json` (`RuntimeConfig`: `pid` i32, `boot_id`, `status` Active/Stopped/Exited/Error, `ip_address`, `restart_policy`, `is_detached`, `args`, `cpu_limit`, `memory_limit`, `rm`, names, workdir) plus runtime artifacts: `upper/`, `work/`, `rootfs/` (overlayfs), and for detached containers `pid`, `container.log`, `error.log`.
- Restart policies (`RestartPolicy`): Never (default), OnFailure, UnlessStopped, Always; evaluated by `autostart.rs` on boot.
- IPAM leases in `<base>/ipam.json`; subnet `172.19.0.0/16` (sometimes written `172.19.0.1/16`) is hardcoded across `bin/crabtainer.rs`, `container.rs`, `refresh.rs`, `autostart.rs` and `start.rs`. Cgroup base `/sys/fs/cgroup` is hardcoded in `cgroups.rs` and the `stop` CLI path. Bridge name `crabtainer0` is hardcoded in `container.rs`.
- `boot_id` (`/proc/sys/kernel/random/boot_id`) is stored per container so `refresh` can detect stale pids after a host reboot.

## Testing conventions

- `src/engine/support/test_utils.rs` provides `with_home`/`without_home` to override `CRABTAINER_HOME`; they serialize behind a global `Mutex` (`ENV_LOCK`). Never set `CRABTAINER_HOME` directly in a test.
- Keep tests hermetic: use `tempdir()` for fs/cgroup work and fake env for paths. Do not touch real `/sys/fs/cgroup`, create network namespaces, or spawn containers in tests — CI is unprivileged.

## Crabtainerfile

Instructions: `DOWNLOAD <image> AS <alias>` (Docker Hub), `FROM`, `COPY`, `RUN`, `CMD`, `CPU_LIMIT <cores>`, `MEMORY_LIMIT <size>` (`512m`/`2g` style suffixes parsed by `parse_memory_limit`). `COPY` honors `.crabtainerignore` (globset).

## Commit style

Imperative, short titles ("Add exec command and implement exec", "Fix ..."), occasionally with a body summarizing the change.
