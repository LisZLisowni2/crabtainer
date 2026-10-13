//! Implementations of the individual build steps a `Crabtainerfile` can
//! contain.
//!
//! - [`download`] — pull an image from a registry (Docker Hub) into the image
//!   store (`DOWNLOAD`).
//! - [`from`] — unpack a stored image into a layout's rootfs (`FROM`).
//! - [`copy`] — copy files from the build directory into the layout, honoring
//!   `.crabtainerignore` (`COPY`).
//! - [`run`] — execute a shell command inside the layout's rootfs (`RUN`).

pub mod copy;
pub mod download;
pub mod from;
pub mod run;
