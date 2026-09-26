//! Running containers.
//!
//! - [`options`] — shared option/state types used by both the runtime
//!   orchestrator and the resource controllers.
//! - [`container`] — container lifecycle: overlayfs mount, namespace
//!   creation, pivot root, and process exec.
//! - [`cgroups`] — cgroup v2 resource limits and process attachment.
//! - [`network`] — bridge networking and IP allocation (IPAM).
//! - [`autostart`] - manage autostarting after computer restarts
//! - [`exec`] - execute command inside container
//! - [`refresh`] - check status of containers (mainly after PC's restart)
//! - [`start`] - start the stopped container
//! - [`stop`] - stop the active container
//! - [`options`] - specifications for container and restart policy

pub mod autostart;
pub mod cgroups;
pub mod container;
pub mod exec;
pub mod network;
pub mod options;
pub mod refresh;
pub mod start;
pub mod stop;
