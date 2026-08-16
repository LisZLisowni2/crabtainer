use crate::engine::runtime::network::Ipam;
use crate::engine::runtime::options::RuntimeConfig;
use nix::mount::{MntFlags, umount2};
use std::fs;
use std::path::PathBuf;

pub async fn stop_container(
    ipam: Ipam,
    container_id: &String,
    container_pid: i32,
    cgroup_dir: &PathBuf,
    container_workdir: PathBuf,
    runtime_config: &mut RuntimeConfig,
) -> Result<(), String> {
    let merged_rootfs = container_workdir.join("rootfs");

    let released_ip = ipam
        .release(container_id)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "[IPAM] Released IP for container {}: {}",
        &container_id, released_ip
    );

    if let Err(e) = fs::remove_dir(cgroup_dir) {
        eprintln!("[WARN] Error during cgroup deletion: {}", e);
    }

    println!("[HOST] Umount overlayfs");

    if let Err(e) = umount2(&merged_rootfs, MntFlags::MNT_DETACH) {
        eprintln!("[WARN] Failed to umount overlayfs: {}", e);
    }

    runtime_config.status = crate::engine::runtime::options::ContainerStatus::Stopped;

    fs::write(
        container_workdir.join("config.json"),
        serde_json::to_string_pretty(&runtime_config)
            .unwrap()
            .into_bytes(),
    )
    .unwrap();

    println!("[HOST] Killing a process");

    let pid = nix::unistd::Pid::from_raw(container_pid);

    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL)
        .expect("[ERROR] Failed to kill container");

    println!("[HOST] Container stopped");

    Ok(())
}
