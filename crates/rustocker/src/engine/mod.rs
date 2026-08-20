//! Core container engine.
//!
//! The engine is split into three concerns:
//!
//! - [`build`] — the Rustockerfile DSL, instruction execution, and OCI spec
//!   generation that together turn a Rustockerfile into a buildable layout.
//! - [`runtime`] — running containers: lifecycle orchestration, cgroup limits,
//!   networking/IPAM, and the shared option types.
//! - [`support`] — low-level infrastructure shared by both: filesystem layout
//!   and test utilities.

pub mod build;
pub mod runtime;
pub mod support;
