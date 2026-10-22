//! Building layouts from a Crabtainerfile.
//!
//! - [`crabtainerfile`] — the instruction model and Crabtainerfile parser.
//! - [`spec`] — generation of the OCI runtime spec (config.json) for a layout.
//! - [`builder`] — orchestration: walks the parsed instructions and executes
//!   each build step.
//! - [`instructions`] — one module per build step (`DOWNLOAD`, `FROM`, `COPY`,
//!   `RUN`).
//!   [`publish`] - handles conversion from layout to image

pub mod builder;
pub mod crabtainerfile;
pub mod instructions;
pub mod publish;
pub mod spec;
