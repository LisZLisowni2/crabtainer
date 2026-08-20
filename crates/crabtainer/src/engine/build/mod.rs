//! Building layouts from a Crabtainerfile.
//!
//! - [`crabtainerfile`] — the instruction model and Crabtainerfile parser.
//! - [`spec`] — generation of the OCI runtime spec (config.json) for a layout.
//! - [`builder`] — orchestration: walks the parsed instructions and executes
//!   each build step.
//! - [`instructions`] — one module per build step (`DOWNLOAD`, `FROM`, `COPY`,
//!   `RUN`).

pub mod builder;
pub mod instructions;
pub mod crabtainerfile;
pub mod spec;
