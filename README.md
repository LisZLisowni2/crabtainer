# Rustocker

Rustocker is the daemonless container engine written from scratch in Rust.
Objectives are to achive lightweight and fast image builder and runtime.

## Features

- OCI compatible image specs
- Own runtime program
- Management of images and layouts (ready to use in containers)
- Functional Rustockerfile and .rustockerignore
- CPU/memory limits via cgroups v2

## How to run

### Requirements
- Linux
- Cgroups v2
- Rust toolchain
- root/sudo privileges
- linux namespaces & overlayfs support

### Building

```bash
cargo build --release
```

### Running

```bash
sudo ./target/release/rustocker
```

## LICENSE

The license for this project is based on dual license (APACHE 2.0 and MIT)

## Other

Look also on [security](SECURITY.md), [code of conduct](CODE_OF_CONDUCT.md) and [contributing](CONTRIBUTING.md) files for further information.
