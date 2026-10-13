//! # Crabtainer
//!
//! A daemonless Linux container engine written from scratch in Rust.
//!
//! This library exposes a single entry point, the [`engine`] module, which is
//! split into three concerns:
//!
//! - [`build`](engine::build) — turning a `Crabtainerfile` into a buildable
//!   layout: instruction parsing, per-step execution, and OCI spec generation.
//! - [`runtime`](engine::runtime) — running containers: lifecycle
//!   orchestration, cgroups, networking/IPAM, autostart, exec, and stop.
//! - [`support`](engine::support) — shared low-level infrastructure: on-disk
//!   store layout and test utilities.
//!
//! The companion `crabtainer_init` binary acts as PID 1 inside each container
//! and is exec'd via `/dev/.crabtainer_init`.
//!
//! See the `crabtainer` binary (`src/bin/crabtainer.rs`) for the CLI that
//! drives this library.

pub mod engine;
