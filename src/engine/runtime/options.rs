use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ContainerOptions {
    pub layout_name: String,
    pub args: Vec<String>,
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<f64>,
    pub container_name: Option<String>,
    pub rm: bool,
}

#[derive(Debug)]
pub struct ContainerReady {
    pub layout_name: String,
    pub args: Vec<String>,
    pub quota: Option<i64>,
    pub memory_limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum ContainerStatus {
    Active,
    Stopped,
    Exited,
}

impl std::fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub layout_name: String,
    pub container_name: String,
    pub status: ContainerStatus,
    pub workdir: PathBuf,
    pub ip_address: Ipv4Addr,
    pub pid: i32,
    pub boot_id: String,
}
