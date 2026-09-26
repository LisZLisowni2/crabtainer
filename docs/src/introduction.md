# Introduction to Crabtainer

**Crabtainer** is a lightweight, daemonless container engine written from scratch in Rust.

## Key Features

- **Daemonless:** No background service. Every container is managed directly by its CLI invocation or init child.
- **Resource Limits:** Fine-grained CPU and Memory isolation powered by Linux `cgroups v2`.
- **Custom Image Builder:** Build reproducible rootfs layouts using declarative `Crabtainerfile` specs.
- **Fast & Safe:** Built with Rust's memory safety and zero-cost abstractions.
