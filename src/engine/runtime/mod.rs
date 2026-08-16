//! Running containers.
//!
//! - [`options`] — shared option/state types used by both the runtime
//!   orchestrator and the resource controllers.
//! - [`container`] — container lifecycle: overlayfs mount, namespace
//!   creation, pivot root, and process exec.
//! - [`cgroups`] — cgroup v2 resource limits and process attachment.
//! - [`network`] — bridge networking and IP allocation (IPAM).

pub mod cgroups;
pub mod container;
pub mod network;
pub mod options;
pub mod exec;
pub mod stop;
pub mod refresh;
