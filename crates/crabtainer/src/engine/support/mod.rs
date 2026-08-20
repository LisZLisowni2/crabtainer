//! Low-level infrastructure shared by the build and runtime groups.
//!
//! - [`paths`] — the on-disk layout of the crabtainer stores and runtime dirs.
//! - [`test_utils`] — helpers for tests that mutate the environment (only
//!   compiled under `cfg(test)`).

pub mod paths;

#[cfg(test)]
pub mod test_utils;
pub mod systemd;
