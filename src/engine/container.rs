use crate::engine::cgroups::{attach_process_to_cgroup, setup_cgroups};
use crate::engine::paths::RustockerPaths;
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, clone};
use nix::sys::signal::Signal;
use nix::unistd::{chdir, execvp, sethostname};
use rand::RngExt;
use std::ffi::CString;
use std::fs;
use std::path::Path;
use clap::arg;
use oci_spec::runtime::Spec;

#[derive(Debug)]
pub struct ContainerOptions {
    pub layout_name: String,
    pub args: Vec<String>,
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<f64>,
}

#[derive(Debug)]
pub struct ContainerReady {
    pub layout_name: String,
    pub args: Vec<String>,
    pub quota: Option<i64>,
    pub memory_limit: Option<i64>,
}

pub(crate) fn generate_container_id() -> String {
    let mut rng = rand::rng();

    let bytes: [u8; 6] = rng.random();

    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub(crate) fn resolve_args(args: &[String], layout_args: &Vec<String>) -> Vec<String> {
    if args.is_empty() {
        layout_args.to_vec()
    } else {
        args.to_vec()
    }
}

pub(crate) fn resolve_cpu_limit(cpu_limit: &Option<f64>, layout_cpu_limit: Option<i64>) -> i64 {
    if let Some(cpu) = cpu_limit {
        (*cpu as i64) * 100000i64
    } else {
        if let Some(cpu) = layout_cpu_limit {
            cpu
        } else {
            100000
        }
    }
}

pub(crate) fn resolve_memory_limit(memory_limit: &Option<f64>, layout_memory_limit: Option<i64>) -> i64 {
    if let Some(memory) = memory_limit {
        *memory as i64
    } else {
        if let Some(memory) = layout_memory_limit {
            memory
        } else {
            100000
        }
    }
}

pub async fn run_container(opts: ContainerOptions) -> Result<(), String> {
    const STACK_SIZE: usize = 5 * 1024 * 1024; // 5 MB
    println!("[HOST] Running a container...");

    let layout_dir = RustockerPaths::layout_store_dir().join(&opts.layout_name);
    if !layout_dir.exists() {
        return Err(format!(
            "Layout '{}' doesn't exist! Build it first using 'rustocker build'.",
            opts.layout_name
        ));
    }
    let layout_rootfs = layout_dir.join("rootfs");

    let layout_opts: oci_spec::runtime::Spec = Spec::load(layout_dir.join("config.json")).unwrap();

    let default_args = layout_opts
        .process()
        .as_ref()
        .and_then(|p| p.args().as_ref());

    let default_cpu = layout_opts
        .linux()
        .as_ref()
        .and_then(|l| l.resources().as_ref())
        .and_then(|r| r.cpu().as_ref())
        .and_then(|c| c.quota());

    let default_memory = layout_opts
        .linux().as_ref()
        .and_then(|l| l.resources().as_ref())
        .and_then(|r| r.memory().as_ref())
        .and_then(|m| m.limit());


    let args = resolve_args(&opts.args, default_args.unwrap());

    let cpu_limit = resolve_cpu_limit(
        &opts.cpu_limit,
        default_cpu
    );

    let memory_limit = resolve_memory_limit(
        &opts.memory_limit,
        default_memory,
    );

    let container_id = generate_container_id();

    let container_workdir = RustockerPaths::runtime_dir().join(&container_id);

    let upper_dir = container_workdir.join("upper");
    let work_dir = container_workdir.join("work");
    let merged_rootfs = container_workdir.join("rootfs");

    fs::create_dir_all(&upper_dir).map_err(|e| format!("Upperdir failed to create: {}", e))?;
    fs::create_dir_all(&work_dir).map_err(|e| format!("Workdir failed to create: {}", e))?;
    fs::create_dir_all(&merged_rootfs).map_err(|e| format!("Rootfs failed to create: {}", e))?;

    let overlay_opts = format!(
        "lowerdir={},upperdir={},workdir={}",
        layout_rootfs.to_str().unwrap(),
        upper_dir.to_str().unwrap(),
        work_dir.to_str().unwrap()
    );

    println!("[HOST] Mount overlayfs");

    mount(
        Some("overlay"),
        &merged_rootfs,
        Some("overlay"),
        MsFlags::empty(),
        Some(overlay_opts.as_str()),
    )
    .expect("[ERROR] Failed to mount overlayfs");

    println!("[HOST] Starting container {}", container_id);

    let final_opts = ContainerReady {
        layout_name: opts.layout_name,
        args,
        quota: Some(cpu_limit),
        memory_limit: Some(memory_limit),
    };

    let cgroup_dir = setup_cgroups(&container_id, &final_opts).await?;

    let proc_dir = merged_rootfs.join("proc");
    fs::create_dir_all(&proc_dir).ok();

    let mut stack: Vec<u8> = vec![0u8; STACK_SIZE];

    let flags = CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS;

    let child_pid = unsafe {
        clone(
            Box::new(|| child_process(&merged_rootfs, &container_id, &final_opts)),
            &mut stack[..],
            flags,
            Some(Signal::SIGCHLD as i32),
        )
        .expect("[ERROR] Failed to spawn child.")
    };

    if let Err(e) = attach_process_to_cgroup(&cgroup_dir, child_pid).await {
        eprintln!("[WARN] Failed to attach process to cgroup: {}", e);
    };

    tokio::task::spawn_blocking(move || {
        nix::sys::wait::waitpid(child_pid, None).unwrap();
    })
    .await
    .map_err(|e| format!("[ERROR] Error waiting for child process: {}", e))?;

    if let Err(e) = fs::remove_dir(&cgroup_dir) {
        eprintln!("[WARN] Error during cgroup deletion: {}", e);
    }

    println!("[HOST] Umount overlayfs");

    if let Err(e) = umount2(&merged_rootfs, MntFlags::MNT_DETACH) {
        eprintln!("[WARN] Failed to umount overlayfs: {}", e);
    }

    fs::remove_dir_all(&container_workdir)
        .map_err(|e| format!("[WARN] Failed to delete container workdir: {}", e))?;

    println!("[HOST] Container stopped");

    Ok(())
}

fn child_process(rootfs: &Path, container_id: &String, options: &ContainerReady) -> isize {
    sethostname(container_id).ok();

    if let Err(e) = mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    ) {
        eprintln!("[CHILD ERROR] Failed to mount / on MS_PRIVATE: {}", e);
        return 1;
    }

    if let Err(e) = mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    ) {
        eprintln!("[CHILD ERROR] Failed to mount old_root: {}", e);
        return 1;
    }

    let old_root_dir = rootfs.join(".oldroot");
    if let Err(e) = fs::create_dir_all(&old_root_dir) {
        eprintln!("[CHILD ERROR] Failed to create old root: {}", e);
        return 1;
    }

    if let Err(e) = chdir(rootfs) {
        eprintln!("[CHILD ERROR] Failed to chdir root: {}", e);
        return 1;
    }

    if let Err(e) = nix::unistd::pivot_root(".", ".oldroot") {
        eprintln!("[CHILD ERROR] Failed to pivot root: {}", e);
        return 1;
    }

    if let Err(e) = chdir("/") {
        eprintln!("[CHILD ERROR] Failed to chdir root: {}", e);
        return 1;
    }

    let proc_target = Path::new("/proc");

    if let Err(e) = mount(
        Some("proc"),
        proc_target,
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    ) {
        eprintln!("[CHILD ERROR] Failed to mount proc: {}", e);
        return 1;
    };

    umount2("/.oldroot", MntFlags::MNT_DETACH)
        .map_err(|e| eprintln!("[CHILD WARN] Failed to umount .oldroot: {}", e))
        .ok();

    let old_root_path = Path::new("/.oldroot");
    if let Err(e) = fs::remove_dir_all(old_root_path) {
        eprintln!("[WARN] Failed to remove old root: {}", e);
    }

    let cmd_cstring = CString::new(options.args[0].clone()).unwrap();
    let mut args_cstring = vec![cmd_cstring.clone()];
    for arg in &options.args[1..] {
        args_cstring.push(CString::new(arg.clone()).unwrap());
    }

    execvp(&cmd_cstring, &args_cstring).ok();

    0
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
    fn resolve_args_layout_args_when_command_args_empty() {
        let layout_args: Vec<String> = vec!["-lh".to_string()];
        assert_eq!(resolve_args(&vec![], &layout_args), vec!["-lh"]);
    }

    #[test]
    fn resolve_args_empty_layout_args_and_empty_command_args() {
        assert!(resolve_args(&vec![], &vec![]).is_empty());
    }

    #[test]
    fn resolve_args_prefers_provided_args() {
        let args: Vec<String> = vec!["-lh".to_string()];
        assert_eq!(resolve_args(&vec!["aux".to_string()], &args), vec!["aux"]);
    }
}
