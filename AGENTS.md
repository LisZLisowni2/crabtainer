# AGENTS.md

Daemonless Linux container engine (image builder + runtime) in Rust, edition 2024 (needs Rust 1.85+, repo built with 1.96).

## Commands

- `cargo build` / `cargo test` — all unit tests are unprivileged-safe (tempdir + fake env), so `cargo test` works without root.
- `cargo fmt` — run after edits; there is no rustfmt config, defaults apply. (Repo history contains a dedicated "Cargo fmt" commit.)
- `cargo clippy -- -D warnings` — required by CONTRIBUTING.md before PRs.
- CI (`.github/workflows/rust.yaml`) only runs `cargo build` + `cargo test` on PRs to `main`.

## Runtime requirements (all commands)

The binary refuses to start without root (`is_root` check in `src/bin/rustocker.rs`). Every container run needs: Linux, cgroups v2, overlayfs, namespaces, sudo. Runtime behavior can't be smoke-tested from an unprivileged agent shell — rely on `cargo test` for verification and keep new tests root-free.

## Layout

- `src/bin/rustocker.rs` — the only binary, CLI + main. It's clap-derive; changing subcommands means updating the `Commands` enum and its arg-parsing tests in the same file.
- `src/engine/build/` — Rustockerfile parser, image download (`oci-client` from Docker Hub), layout build.
- `src/engine/runtime/` — container lifecycle (`container.rs`), exec, network (`network.rs`, rtnetlink veth setup), cgroups, options, plus `refresh.rs` (reap dead containers) and `stop.rs`.
- `src/engine/support/paths.rs` — `RustockerPaths`; `RUSTOCKER_HOME` env var overrides the default base dir `/var/lib/rustocker`.

## State

- Base dir subdirs: `images/`, `layouts/`, `containers/`. Initialized by `RustockerPaths::init_system_dirs()` (called from image download).
- One directory per container under `containers/<id>/` containing `config.json` (`RuntimeConfig`: `pid` i32, `boot_id`, `status` Active/Stopped/Exited, `ip_address`, names, workdir).
- IPAM leases in `<base>/ipam.json`; subnet `172.19.0.0/16` is hardcoded in `src/bin/rustocker.rs` and `refresh.rs`. Cgroup base `/sys/fs/cgroup` is hardcoded in `cgroups.rs` and the `stop` CLI path.
- `boot_id` (`/proc/sys/kernel/random/boot_id`) is stored per container so `refresh` can detect stale pids after a host reboot.

## Testing conventions

- `src/engine/support/test_utils.rs` provides `with_home`/`without_home` to override `RUSTOCKER_HOME`; they serialize behind a global `Mutex`. Never set `RUSTOCKER_HOME` directly in a test.
- Keep tests hermetic: use `tempdir()` for fs/cgroup work and fake env for paths. Do not touch real `/sys/fs/cgroup`, create network namespaces, or spawn containers in tests — CI is unprivileged.

## Rustockerfile

Instructions: `DOWNLOAD <image> AS <alias>` (Docker Hub), `FROM`, `COPY`, `RUN`, `CMD`, `CPU_LIMIT <cores>`, `MEMORY_LIMIT <size>`. `COPY` honors `.rustockerignore` (globset).

## Commit style

Imperative, short titles ("Add exec command and implement exec", "Fix ..."), occasionally with a body summarizing the change.
