//! Shared option and state types used across the runtime.
//!
//! [`ContainerOptions`] is the user-supplied specification for a container
//! (from the CLI or autostart), [`RuntimeConfig`] is the persisted state
//! written to `containers/<id>/config.json`, and [`ContainerReady`] is the
//! resolved combination of both handed to the resource controllers.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;

/// User-supplied options describing how a container should be created.
#[derive(Debug)]
pub struct ContainerOptions {
    /// Name of the layout the container runs from.
    pub layout_name: String,
    /// Command and arguments to run. Empty means "use the layout's default".
    pub args: Vec<String>,
    /// Optional CPU limit in cores (e.g. `1.5`); overrides the layout value.
    pub cpu_limit: Option<f64>,
    /// Optional memory limit in bytes; overrides the layout value.
    pub memory_limit: Option<f64>,
    /// Optional human-friendly name; if `None` the container id is used.
    pub container_name: Option<String>,
    /// Restart policy applied when the container exits.
    pub restart_policy: RestartPolicy,
    /// Whether the container directory should be removed after exit.
    pub rm: bool,
    /// Port forwarding specs in `host:container` form, possibly ranges.
    pub ports: Vec<String>,
    /// Environment variables (`KEY=VALUE`) to set inside the container.
    pub envs: Vec<String>,
    /// Volumes or host paths to bind-mount into the container.
    pub volumes: Vec<String>,
}

/// Resolved runtime configuration passed to the cgroup setup after defaults
/// from the layout's OCI spec have been applied.
#[derive(Debug)]
pub struct ContainerReady {
    /// Name of the layout the container runs from.
    pub layout_name: String,
    /// Resolved command and arguments to execute.
    pub args: Vec<String>,
    /// CPU quota (in microseconds per period); always present after resolve.
    pub quota: Option<i64>,
    /// Memory limit in bytes; always present after resolve.
    pub memory_limit: Option<i64>,
    /// Restart policy applied when the container exits.
    pub restart_policy: RestartPolicy,
    /// Working directory inside the container.
    pub workdir: String,
    /// Environment variables (`KEY=VALUE`) to set inside the container.
    pub envs: Vec<String>,
}

/// Lifecycle status of a container, persisted in its `config.json`.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub enum ContainerStatus {
    /// The container's process is currently running.
    Active,
    /// The container was explicitly stopped (e.g. via `stop` or SIGTERM
    /// resulting in exit code 143).
    Stopped,
    /// The container's process ended on its own.
    Exited,
    /// The container ended with an error (non-zero exit or signal).
    Error,
}

impl std::fmt::Display for ContainerStatus {
    /// Formats the status using its [`Debug`] representation (e.g. `Active`).
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Full persisted state of one container, serialized to
/// `containers/<id>/config.json`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeConfig {
    /// Name of the layout the container runs from.
    pub layout_name: String,
    /// Human-friendly container name (falls back to the container id).
    pub container_name: String,
    /// Current lifecycle status of the container.
    pub status: ContainerStatus,
    /// Restart policy applied when the container exits.
    pub restart_policy: RestartPolicy,
    /// Working directory inside the container.
    pub workdir: PathBuf,
    /// IP address assigned to the container from IPAM.
    pub ip_address: Ipv4Addr,
    /// Memory limit in bytes.
    pub memory_limit: i64,
    /// CPU quota in microseconds per 100 ms period.
    pub cpu_limit: i64,
    /// Resolved command and arguments of the container.
    pub args: Vec<String>,
    /// Host PID of the container's root process (`0` when not running).
    pub pid: i32,
    /// Host boot id at the time the container was started, used by
    /// [`refresh`](crate::engine::runtime::refresh) to detect stale PIDs
    /// after a reboot.
    pub boot_id: String,
    /// Whether the container runs detached (`true`) or foregrounded.
    pub is_detached: bool,
    /// Whether the container directory is removed after exit.
    pub rm: bool,
    /// Port forwarding specs in `host:container` form.
    pub ports: Vec<String>,
    /// Environment variables (`KEY=VALUE`) set inside the container.
    pub envs: Vec<String>,
    /// Volumes or host paths bind-mounted into the container.
    pub volumes: Vec<String>,
}

/// Policy deciding whether a container is restarted after it exits or the
/// host reboots.
#[derive(Debug, Default, Serialize, Deserialize, Clone, ValueEnum, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Never restart the container (default).
    #[default]
    Never,
    /// Restart only if the container exited with an error.
    OnFailure,
    /// Restart on boot unless the container was explicitly stopped.
    UnlessStopped,
    /// Always restart on exit and on boot unless explicitly stopped.
    Always,
}
