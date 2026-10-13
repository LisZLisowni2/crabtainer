//! The `RUN` build step: executing a shell command inside the layout.
//!
//! `RUN` chroots into the layout's rootfs and runs the command with `/bin/sh
//! -c`. Requires root; pseudo-filesystems are mounted around the chroot for
//! the duration of the command and unmounted afterwards.

use crate::engine::support::paths::CrabtainerPaths;
use nix::mount::{MsFlags, mount, umount};
use nix::unistd::{chdir, chroot};
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Mounts `proc`, `sysfs`, and a bind of host `/dev` into the rootfs so the
/// chrooted command has working pseudo-filesystems.
///
/// # Errors
///
/// Returns an error string if any mount fails.
async fn pseudo_filesystems_mount(rootfs: &std::path::Path) -> Result<(), String> {
    mount(
        Some("proc"),
        &rootfs.join("proc"),
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    mount(
        Some("sysfs"),
        &rootfs.join("sys"),
        Some("sysfs"),
        MsFlags::empty(),
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    mount(
        Some("/dev"),
        &rootfs.join("dev"),
        None::<&str>,
        MsFlags::MS_BIND,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Unmounts the `proc`, `sys`, and `dev` mounts previously created by
/// [`pseudo_filesystems_mount`].
///
/// Unmount failures are treated as fatal via `expect`.
async fn pseudo_filesystems_umount(rootfs: &std::path::Path) -> Result<(), String> {
    let targets = [
        (rootfs.join("proc")),
        (rootfs.join("sys")),
        (rootfs.join("dev")),
    ];

    for target in targets {
        umount(&target)
            .map_err(|e| format!("Failed to umount target {}: {}", target.display(), e))
            .expect("Failed to run umount");
    }
    Ok(())
}

/// Runs a shell command inside the layout's rootfs.
///
/// Writes a static `resolv.conf` (pointing at 1.1.1.1 / 8.8.8.8) into the
/// rootfs, mounts the pseudo-filesystems, chroots into the rootfs, executes
/// `sh -c <command>` from the given workdir, and unmounts afterwards.
///
/// # Arguments
///
/// * `output_layout_name` - Layout whose `rootfs/` is the chroot target.
/// * `workdir` - Working directory inside the rootfs (`/` when `None`).
/// * `command` - Shell command line to execute.
///
/// # Errors
///
/// Returns an error string if the resolv.conf cannot be written or the
/// command exits non-zero or cannot be spawned.
pub async fn run_in_container(
    output_layout_name: &String,
    workdir: Option<String>,
    command: String,
) -> Result<(), String> {
    let rootfs_path = CrabtainerPaths::layout_store_dir()
        .join(output_layout_name)
        .join("rootfs");

    let resolv_path = rootfs_path.join("etc").join("resolv.conf");

    if let Err(e) = std::fs::write(
        &resolv_path,
        "nameserver 1.1.1.1\nnameserver 8.8.8.8\n".as_bytes(),
    ) {
        return Err(e.to_string());
    }

    let workdir_final: String = if let Some(dir) = workdir {
        dir
    } else {
        "/".to_string()
    };

    pseudo_filesystems_mount(&rootfs_path).await?;
    let rootfs_path_clone = rootfs_path.clone();

    let status = unsafe {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .pre_exec(move || {
                chdir(rootfs_path.as_path())?;
                chroot(".")?;
                chdir(workdir_final.as_str())?;
                Ok(())
            })
            .status()
    };

    pseudo_filesystems_umount(&rootfs_path_clone).await?;

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("[ERROR] Command exited with status: {}", s)),
        Err(e) => Err(format!("[ERROR] Failed to run command: {}", e)),
    }
}
