# Crabtainer

Crabtainer is the daemonless container engine written from scratch in Rust.
Objectives are to achive lightweight and fast image builder and runtime.

## Features

- OCI compatible image specs
- Own runtime program
- Management of images and layouts (ready to use in containers)
- Functional Crabtainerfile and .crabtainergnore
- CPU/memory limits via cgroups v2
- Complete isolation of containers from host

## How to run

### Requirements
- Linux
- Cgroups v2
- Rust toolchain
- root/sudo privileges
- linux namespaces & overlayfs support
- systemd (optional, only for autostart script)

### Building

```bash
cargo build --release
```

### Running

```bash
sudo ./target/release/core
```

## LICENSE

The license for this project is based on APACHE 2.0 license. Look at [license](LICENSE) for further information.

## Contribution and AI policy

You're welcome if you want to contribute. Look at [CONTRIBUTING.md](CONTRIBUTING.md) for further information. 

## Other

Look also on [security](SECURITY.md), [code of conduct](CODE_OF_CONDUCT.md) and [contributing](CONTRIBUTING.md) files for further information.
