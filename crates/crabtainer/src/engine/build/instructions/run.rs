use crate::engine::support::paths::CrabtainerPaths;
use nix::mount::{MsFlags, mount, umount};
use nix::unistd::{chdir, chroot};
use std::os::unix::process::CommandExt;
use std::process::Command;

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
