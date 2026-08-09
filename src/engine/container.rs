use nix::mount::{mount, umount2, MsFlags, MntFlags};
use nix::sched::{clone, CloneFlags};
use nix::sys::signal::Signal;
use nix::unistd::{chroot, chdir, execvp, sethostname};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use nix::NixPath;
use rand::{Rng, RngExt};
use crate::engine::paths::RustockerPaths;

#[derive(Debug)]
pub struct ContainerOptions {
    pub layout_name: String,
    pub command: String,
    pub args: Vec<String>,
}

pub(crate) fn generate_container_id() -> String {
    let mut rng = rand::rng();

    let bytes: [u8; 6] = rng.random();

    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub(crate) fn resolve_command(command: &str, layout_cmd: &[String]) -> String {
    if command.is_empty() {
        layout_cmd.join(" ")
    } else {
        command.to_string()
    }
}

pub async fn run_container(opts: ContainerOptions) -> Result<(), String> {
    const STACK_SIZE: usize = 5 * 1024 * 1024; // 5 MB
    println!("[HOST] Running a container...");

    let layout_dir = RustockerPaths::layout_store_dir().join(&opts.layout_name);
    if !layout_dir.exists() {
        return Err(format!(
            "Layout '{}' doesn't exist! Build it first using 'rocker build'.",
            opts.layout_name
        ));
    }

    let layout_opts: crate::engine::builder::LayoutOpts = serde_json::from_str(fs::read_to_string(layout_dir.join("config.json")).unwrap().as_str()).unwrap();
    let command = resolve_command(&opts.command, &layout_opts.cmd);

    let container_id = generate_container_id();

    let container_workdir = RustockerPaths::runtime_dir()
        .join(&container_id);

    let upper_dir = container_workdir.join("upper");
    let work_dir = container_workdir.join("work");
    let merged_rootfs = container_workdir.join("rootfs");

    fs::create_dir_all(&upper_dir).map_err(|e| format!("Upperdir failed to create: {}", e))?;
    fs::create_dir_all(&work_dir).map_err(|e| format!("Workdir failed to create: {}", e))?;
    fs::create_dir_all(&merged_rootfs).map_err(|e| format!("Rootfs failed to create: {}", e))?;

    let overlay_opts = format!(
        "lowerdir={},upperdir={},workdir={}",
        layout_dir.to_str().unwrap(),
        upper_dir.to_str().unwrap(),
        work_dir.to_str().unwrap()
    );

    mount(
        Some("overlay"),
        &container_workdir,
        Some("overlay"),
        MsFlags::empty(),
        Some(overlay_opts.as_str()),
    )
        .map_err(|e| format!("Error during mounting OverlayFS: {}", e));

    println!("[HOST] Starting container {}", container_id);

    let proc_dir = merged_rootfs.join("proc");
    println!("[DEBUG] {}", proc_dir.display());
    fs::create_dir_all(&proc_dir).ok();

    let mut stack: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS;

    let final_opts = ContainerOptions {
        layout_name: opts.layout_name,
        args: opts.args,
        command
    };

    let child_pid = unsafe {
        clone(
            Box::new(|| child_process(&merged_rootfs, &final_opts)),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32)
        )
            .expect("Failed to spawn child.")
    };

    nix::sys::wait::waitpid(child_pid, None).unwrap();

    println!("[HOST] Umount overlayfs {}", container_id);

    if let Err(e) = umount2(&container_workdir, MntFlags::MNT_DETACH) {
        eprintln!("[WARN] Failed to umount overlayfs: {}", e);
    }

    let _ = fs::remove_dir_all(&container_workdir);

    println!("[HOST] Container stopped");

    Ok(())
}

fn child_process(rootfs: &Path, options: &ContainerOptions) -> isize {
    let _ = sethostname(&options.layout_name);

    let proc_target = rootfs.join("proc");

    let _ = mount(
        Some("proc"),
        &proc_target,
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    );

    if let Err(e) = chroot(rootfs) {
        eprintln!("Chroot error: {}", e);
        return 1;
    }

    if let Err(e) = chdir("/") {
        eprintln!("chdir error: {}", e);
        return 1;
    }

    let status: ExitStatus = Command::new(&options.command)
        .args(&options.args)
        .status()
        .unwrap_or_else(|_| ExitStatus::default());

    let _ = umount2("/proc", MntFlags::MNT_DETACH);

    if status.success() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_container_id_is_12_lowercase_hex_chars() {
        for _ in 0..100 {
            let id = generate_container_id();
            assert_eq!(id.len(), 12);
            assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn resolve_command_uses_layout_cmd_when_command_empty() {
        let cmd = vec!["/bin/sh".to_string(), "-c".to_string(), "echo hi".to_string()];
        assert_eq!(resolve_command("", &cmd), "/bin/sh -c echo hi");
    }

    #[test]
    fn resolve_command_prefers_provided_command() {
        let cmd = vec!["/bin/sh".to_string(), "-c".to_string()];
        assert_eq!(resolve_command("ls", &cmd), "ls");
    }

    #[test]
    fn resolve_command_empty_cmd_yields_empty_string() {
        assert_eq!(resolve_command("", &[]), "");
    }
}