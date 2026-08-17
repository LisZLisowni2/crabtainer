use crate::engine::support::paths::RustockerPaths;
use nix::unistd::{chroot, chdir};
use std::process::{Command};
use std::os::unix::process::CommandExt;

pub async fn run_in_container(output_layout_name: &String, command: String) -> Result<(), String> {
    println!(" => [RUN] Running '{}' command in container", command);

    let rootfs_path = RustockerPaths::layout_store_dir()
        .join(output_layout_name)
        .join("rootfs");

    let resolv_path = rootfs_path.join("etc").join("resolv.conf");

    if let Err(e) = std::fs::write(
        &resolv_path,
       "nameserver 1.1.1.1\nnameserver 8.8.8.8\n"
           .as_bytes()
    ) {
        return Err(e.to_string());
    }
    
    let status = unsafe {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .pre_exec(move || {
                chroot(rootfs_path.as_path())?;
                chdir("/")?;
                Ok(())
            })
            .status()
    };

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("[ERROR] Command exited with status: {}", s)),
        Err(e) => Err(format!("[ERROR] Failed to run command: {}", e)),
    }
}
