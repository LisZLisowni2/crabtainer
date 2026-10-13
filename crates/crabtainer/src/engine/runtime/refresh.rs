//! Detecting and reconciling dead containers.
//!
//! Called before most CLI operations, this module marks containers whose
//! process is gone (including after a host reboot) as exited, frees their
//! IPAM leases, and unmounts their overlay rootfs.

use crate::engine::runtime::network::Ipam;
use crate::engine::runtime::options::{ContainerStatus, RuntimeConfig};
use crate::engine::support::paths::CrabtainerPaths;
use nix::unistd::Pid;
use std::net::Ipv4Addr;

/// Summary of what [`refresh_container_states`] changed.
#[derive(Debug)]
pub struct RefreshReport {
    /// Container ids that were transitioned from `Active` to `Exited`.
    pub updated_containers: Vec<String>,
    /// IP addresses freed as part of that transition.
    pub freed_ips: Vec<Ipv4Addr>,
}

/// Reads the current host boot id from
/// `/proc/sys/kernel/random/boot_id`.
///
/// The boot id is persisted per container so stale PIDs can be detected after
/// a reboot. Panics if the file cannot be read.
pub fn save_boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .expect("[ERROR] Failed to read /proc/sys/kernel/random/boot_id")
        .trim()
        .to_string()
}

/// Checks whether a container process with `pid` is still alive.
///
/// A PID is considered dead if it is `<= 0`, if the boot id it was recorded
/// under differs from `current_boot_id` (i.e. the host rebooted), or if
/// signalling it with a null signal fails with `ESRCH`.
fn is_container_alive(pid: i32, saved_boot_id: Option<String>, current_boot_id: String) -> bool {
    if pid <= 0 {
        return false;
    }

    if let Some(boot_id) = saved_boot_id
        && boot_id != current_boot_id
    {
        return false;
    }

    match nix::sys::signal::kill(Pid::from_raw(pid), None) {
        Ok(_) => true,
        Err(nix::errno::Errno::ESRCH) => false,
        Err(_) => false,
    }
}

/// Checks the runtime directory for containers marked `Active` whose process
/// is no longer alive and reconciles them.
///
/// For each dead container this:
///
/// 1. writes `status = Exited` and `pid = 0` back to its `config.json`,
/// 2. releases its IPAM lease and records the freed IP in the report,
/// 3. detaches (unmounts) the container's merged overlay rootfs.
///
/// Containers without a readable config are skipped. Returns a
/// [`RefreshReport`] summarizing the changes.
///
/// # Errors
///
/// Returns an error if the runtime directory cannot be enumerated or a
/// config cannot be re-written.
pub async fn refresh_container_states() -> Result<RefreshReport, Box<dyn std::error::Error>> {
    let current_boot_id = save_boot_id();

    let mut report = RefreshReport {
        updated_containers: Vec::new(),
        freed_ips: Vec::new(),
    };

    let container_dir = CrabtainerPaths::runtime_dir();
    if !container_dir.exists() {
        return Ok(report);
    }

    for entry in std::fs::read_dir(&container_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .expect("[ERROR] Failed to get file name")
            .to_str()
            .expect("[ERROR] Failed to convert OsStr to str");
        let config_path = path.join("config.json");

        if !&config_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&config_path)?;
        let mut config: RuntimeConfig = match serde_json::from_str(&content) {
            Ok(cfg) => cfg,
            Err(_) => continue,
        };

        if config.status == ContainerStatus::Active {
            let is_alive = is_container_alive(
                config.pid,
                Some(config.boot_id.clone()),
                current_boot_id.to_string(),
            );

            if !is_alive {
                println!("[REFRESH] Detected a dead container. {}", name);
                config.status = ContainerStatus::Exited;
                config.pid = 0;

                let updated_json = serde_json::to_string_pretty(&config)?;
                std::fs::write(&config_path, &updated_json)?;

                if let Ok(ipam) = Ipam::new(
                    "172.19.0.1/16",
                    CrabtainerPaths::base_dir().join("ipam.json"),
                ) {
                    let _ = ipam.release(&name.to_string()).await;
                    report.freed_ips.push(config.ip_address);
                }

                let merged_rootfs = path.join("rootfs");
                if merged_rootfs.exists() {
                    let _ = nix::mount::umount2(&merged_rootfs, nix::mount::MntFlags::MNT_DETACH);
                }

                report.updated_containers.push(name.to_string());
            }
        }
    }

    Ok(report)
}
