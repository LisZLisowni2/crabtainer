use crate::engine::runtime::cgroups::{attach_process_to_cgroup, setup_cgroups};
use crate::engine::runtime::network::{Ipam, NetworkManager};
use crate::engine::runtime::options::{ContainerOptions, ContainerStatus};
use crate::engine::runtime::refresh::save_boot_id;
use crate::engine::support::paths::CrabtainerPaths;
use nix::fcntl::OFlag;
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, clone};
use nix::sys::signal::{Signal};
use nix::sys::stat::Mode;
use nix::unistd::{
    ForkResult, chdir, dup2_stderr, dup2_stdin, dup2_stdout, execvp, fork, sethostname, setsid,
};
use oci_spec::runtime::Spec;
use rand::RngExt;
use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::net::Ipv4Addr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};

pub fn generate_container_id() -> String {
    let mut rng = rand::rng();

    let bytes: [u8; 6] = rng.random();

    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn resolve_args(args: &[String], layout_args: &[String]) -> Vec<String> {
    if args.is_empty() {
        layout_args.to_vec()
    } else {
        args.to_vec()
    }
}

pub fn resolve_cpu_limit(cpu_limit: &Option<f64>, layout_cpu_limit: Option<i64>) -> i64 {
    if let Some(cpu) = cpu_limit {
        (*cpu as i64) * 100000i64
    } else {
        layout_cpu_limit.unwrap_or(100000)
    }
}

pub fn resolve_memory_limit(memory_limit: &Option<f64>, layout_memory_limit: Option<i64>) -> i64 {
    if let Some(memory) = memory_limit {
        *memory as i64
    } else {
        layout_memory_limit.unwrap_or(1024 * 1024 * 1024) // 1GB default
    }
}

pub async fn spawn_detach_container(
    opts: ContainerOptions,
    container_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[HOST] Spawning container {}...", container_id);

    let bridge_name = "crabtainer0";
    let subnet_mask = "172.19.0.1/16";
    let network_manager =
        NetworkManager::new(bridge_name.to_string(), Ipv4Addr::new(172, 19, 0, 1), 16)
            .await
            .map_err(|e| e.to_string())?;

    let ipam = Ipam::new(subnet_mask, CrabtainerPaths::base_dir().join("ipam.json"))
        .map_err(|e| e.to_string())?;

    network_manager
        .init_global_network()
        .await
        .map_err(|e| e.to_string())?;

    let assigned_ip = ipam
        .allocate(&container_id)
        .await
        .map_err(|e| e.to_string())?;

    let runtime_path = CrabtainerPaths::runtime_dir().join(&container_id);
    fs::create_dir_all(&runtime_path)?;

    tokio::task::spawn_blocking(move || {
        match unsafe { fork() }.expect("[HOST] Failed to fork") {
            ForkResult::Parent { child } => {
                let pid_file = runtime_path.join("pid");
                fs::write(pid_file, child.as_raw().to_string().as_bytes()).unwrap();
                println!("[HOST] PID file written to file. {}", child);
                return Ok(());
            }
            ForkResult::Child => {
                setsid().expect("[HOST] Failed to setsid");

                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();
                let stdin = std::io::stdin();

                let saved_stdout = nix::unistd::dup(&stdout).unwrap();
                let saved_stderr = nix::unistd::dup(&stderr).unwrap();
                let saved_stdin = nix::unistd::dup(&stdin).unwrap();

                let log_fd = nix::fcntl::open(
                    runtime_path.join("container.log").as_path(),
                    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND,
                    Mode::from_bits_truncate(644),
                )
                .expect("[HOST] Failed to open container.log");

                let error_fd = nix::fcntl::open(
                    runtime_path.join("error.log").as_path(),
                    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND,
                    Mode::from_bits_truncate(644),
                )
                .expect("[HOST] Failed to open error.log");

                let dev_null = nix::fcntl::open(
                    PathBuf::from("/dev/null").as_path(),
                    OFlag::O_RDONLY,
                    Mode::empty(),
                )
                .expect("[HOST] Failed to open /dev/null");

                dup2_stdin(&dev_null).expect("[HOST] Failed to dup2 stdin");
                dup2_stdout(&log_fd).expect("[HOST] Failed to dup2 stdout");
                dup2_stderr(&error_fd).expect("[HOST] Failed to dup2 stderr");

                const STACK_SIZE: usize = 5 * 1024 * 1024; // 5 MB
                let layout_dir = CrabtainerPaths::layout_store_dir().join(&opts.layout_name);
                if !layout_dir.exists() {
                    stderr.write_all(format!(
                        "Layout '{}' doesn't exist! Build it first using 'crabtainer build'.\n",
                        opts.layout_name
                    ).as_bytes()).expect("[HOST] Failed to write to error");
                    return Err(format!(
                        "Layout '{}' doesn't exist! Build it first using 'crabtainer build'.",
                        opts.layout_name
                    ));
                }
                let layout_rootfs = layout_dir.join("rootfs");

                let layout_opts: oci_spec::runtime::Spec =
                    Spec::load(layout_dir.join("config.json")).unwrap();

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
                    .linux()
                    .as_ref()
                    .and_then(|l| l.resources().as_ref())
                    .and_then(|r| r.memory().as_ref())
                    .and_then(|m| m.limit());

                let args = resolve_args(&opts.args, default_args.unwrap());

                let cpu_limit = resolve_cpu_limit(&opts.cpu_limit, default_cpu);

                let memory_limit = resolve_memory_limit(&opts.memory_limit, default_memory);

                stdout
                    .write_all(
                        format!(
                            "[IPAM] Assigned IP for container {}: {}\n",
                            &container_id, assigned_ip
                        )
                        .as_bytes(),
                    )
                    .unwrap();

                let container_workdir = CrabtainerPaths::runtime_dir().join(&container_id);

                let upper_dir = container_workdir.join("upper");
                let work_dir = container_workdir.join("work");
                let merged_rootfs = container_workdir.join("rootfs");

                fs::create_dir_all(&upper_dir)
                    .map_err(|e| format!("Upperdir failed to create: {}", e))?;
                fs::create_dir_all(&work_dir)
                    .map_err(|e| format!("Workdir failed to create: {}", e))?;
                fs::create_dir_all(&merged_rootfs)
                    .map_err(|e| format!("Rootfs failed to create: {}", e))?;

                let overlay_opts = format!(
                    "lowerdir={},upperdir={},workdir={}",
                    layout_rootfs.to_str().unwrap(),
                    upper_dir.to_str().unwrap(),
                    work_dir.to_str().unwrap()
                );

                mount(
                    Some("overlay"),
                    &merged_rootfs,
                    Some("overlay"),
                    MsFlags::empty(),
                    Some(overlay_opts.as_str()),
                )
                .expect("[ERROR] Failed to mount overlayfs");

                // Volumes & bind mounts
                let container_init_path = merged_rootfs.join("dev/.crabtainer_init");
                let container_init_localization = std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("crabtainer_init");
                fs::OpenOptions::new().create(true).write(true).open(&container_init_path).expect("[ERROR] Failed to open dev/.crabtainer_init");

                mount(
                    Some(container_init_localization.to_str().unwrap()),
                    &container_init_path,
                    None::<&str>,
                    MsFlags::MS_BIND | MsFlags::MS_RDONLY,
                    None::<&str>,
                ).expect("[ERROR] Failed to mount init program");

                stdout
                    .write_all(format!("[HOST] Starting container {}\n", container_id).as_bytes())
                    .unwrap();

                let final_opts = crate::engine::runtime::options::ContainerReady {
                    layout_name: opts.layout_name.clone(),
                    args: args.clone(),
                    quota: Some(cpu_limit),
                    memory_limit: Some(memory_limit),
                    restart_policy: opts.restart_policy.clone()
                };

                let cgroup_dir = setup_cgroups(&container_id, &final_opts)?;

                let proc_dir = merged_rootfs.join("proc");
                fs::create_dir_all(&proc_dir).ok();

                let mut stack: Vec<u8> = vec![0u8; STACK_SIZE];

                let flags = CloneFlags::CLONE_NEWUTS
                    | CloneFlags::CLONE_NEWPID
                    | CloneFlags::CLONE_NEWNS
                    | CloneFlags::CLONE_NEWNET;

                let (read_fd, write_fd) =
                    nix::unistd::pipe().expect("[HOST] Failed to create pipe");

                let child_pid = unsafe {
                    clone(
                        Box::new(|| {
                            detach_child_process(
                                &merged_rootfs,
                                &container_id,
                                &final_opts,
                                read_fd.as_raw_fd(),
                            )
                        }),
                        &mut stack[..],
                        flags,
                        Some(Signal::SIGCHLD as i32),
                    )
                    .expect("[ERROR] Failed to spawn child.")
                };

                if let Err(e) = attach_process_to_cgroup(&cgroup_dir, child_pid) {
                    eprintln!("[WARN] Failed to attach process to cgroup: {}", e);
                };

                let container_name = if let Some(name) = &opts.container_name {
                    name.clone()
                } else {
                    container_id.clone().to_string()
                };
                let cwd = layout_opts
                    .process()
                    .as_ref()
                    .and_then(|p| p.cwd().to_str())
                    .expect("[ERROR] Failed to get work dir");

                let mut runtime_config = crate::engine::runtime::options::RuntimeConfig {
                    container_name,
                    status: ContainerStatus::Active,
                    layout_name: opts.layout_name.clone(),
                    ip_address: assigned_ip,
                    workdir: PathBuf::from(cwd),
                    pid: child_pid.as_raw(),
                    boot_id: save_boot_id(),
                    restart_policy: opts.restart_policy.clone(),
                    is_detached: true,
                    cpu_limit,
                    memory_limit,
                    args: args.clone(),
                    rm: opts.rm,
                };

                fs::write(
                    container_workdir.join("config.json"),
                    serde_json::to_string_pretty(&runtime_config)
                        .unwrap()
                        .into_bytes(),
                )
                .unwrap();

                let _ = nix::unistd::write(write_fd, b"1");

                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("[ERROR] Failed to create tokio runtime");

                rt.block_on(async {
                    let (conn, handle, _) = rtnetlink::new_connection().unwrap();
                    let conn_handle = tokio::spawn(conn);

                    let setup_res = network_manager
                        .attach_container_with_custom_handle(
                            container_id.as_str(),
                            child_pid.as_raw(),
                            assigned_ip,
                            handle,
                        )
                        .await;

                    conn_handle.abort();

                    if let Err(e) = setup_res {
                        eprintln!("[WARN] Failed to attach container: {}", e);
                    }
                });

                let container_workdir_clone = container_workdir.clone();

                rt.block_on(async move {
                    let wait_result = tokio::task::spawn_blocking(move || {
                        nix::sys::wait::waitpid(child_pid, None)
                    })
                    .await
                    .unwrap();

                    let exit_status: Result<i32, String> = match wait_result {
                        Ok(nix::sys::wait::WaitStatus::Exited(_, status)) => {
                            println!("[INFO] Container exited with code {}", status);
                            Ok(status)
                        }
                        Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                            Err(format!("Container process exited with signal {:?}", sig))
                        }
                        Ok(status) => {
                            println!("[INFO] Container proceed changed state: {:?}", status);
                            Ok(0)
                        }
                        Err(e) => Err(format!("[ERROR] Error waiting for child process: {:?}", e)),
                    };

                    tokio::task::spawn_blocking(move || {
                        runtime_config.status = ContainerStatus::Exited;
                        if let Ok(config) = serde_json::to_string_pretty(&runtime_config) {
                            let _ = fs::write(container_workdir.join("config.json"), config);
                        }

                        if let Err(e) = fs::remove_dir(&cgroup_dir) {
                            eprintln!("[WARN] Error during cgroup deletion: {}", e);
                        }

                        println!("[HOST] Umount overlayfs");
                        if let Err(e) = umount2(&merged_rootfs, MntFlags::empty()) {
                            eprintln!(
                                "[WARN] Standard umount failed ({}), trying MNT_DETACH...",
                                e
                            );
                            if let Err(e_detach) = umount2(&merged_rootfs, MntFlags::MNT_DETACH) {
                                eprintln!("[ERROR] Failed to detach overlayfs: {}", e_detach);
                            }
                        }

                        if opts.rm {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            if let Err(e) = fs::remove_dir_all(&container_workdir).map_err(|e| {
                                format!("[WARN] Failed to delete container workdir: {}", e)
                            }) {
                                eprintln!("[WARN] Failed to delete container workdir: {}", e);
                            }
                        }
                    })
                    .await
                    .unwrap();

                    match ipam.release(&container_id).await {
                        Ok(released_ip) => {
                            println!(
                                "[INFO] Released ip for container {}: {}",
                                &container_id, released_ip
                            );
                        }
                        Err(e) => eprintln!("[WARN] Failed to release IP of container: {}", e),
                    }

                    if let Ok(code) = exit_status {
                        if code != 0 {
                            return Err(format!(
                                "Container process exited with exit code: {}",
                                code
                            ));
                        }
                    } else {
                        exit_status?;
                    }

                    Ok(())
                })
                .unwrap();

                dup2_stdout(&saved_stdout).expect("[ERROR] Failed to restore stdout");
                dup2_stderr(&saved_stderr).expect("[ERROR] Failed to restore stderr");
                dup2_stdin(&saved_stdin).expect("[ERROR] Failed to restore stdin");

                // Kill a child process to prevent zombie process
                if let Ok(pid_str) = fs::read_to_string(container_workdir_clone.join("pid"))
                    && let Ok(pid) = pid_str.parse::<i32>()
                    && let Err(e) =
                        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), Signal::SIGKILL)
                {
                    eprintln!("[ERROR] Failed to kill container: {}", e);
                }
            }
        };

        Ok(())
    })
    .await??;

    Ok(())
}

pub async fn run_container(opts: ContainerOptions, container_id: String) -> Result<(), String> {
    const STACK_SIZE: usize = 5 * 1024 * 1024; // 5 MB
    println!("[HOST] Running a container...");
    let bridge_name = "crabtainer0";
    let subnet_mask = "172.19.0.0/16";
    let network_manager =
        NetworkManager::new(bridge_name.to_string(), Ipv4Addr::new(172, 19, 0, 1), 16)
            .await
            .map_err(|e| e.to_string())?;

    let ipam = Ipam::new(subnet_mask, CrabtainerPaths::base_dir().join("ipam.json"))
        .map_err(|e| e.to_string())?;

    network_manager
        .init_global_network()
        .await
        .map_err(|e| e.to_string())?;

    let layout_dir = CrabtainerPaths::layout_store_dir().join(&opts.layout_name);
    if !layout_dir.exists() {
        return Err(format!(
            "Layout '{}' doesn't exist! Build it first using 'crabtainer build'.",
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
        .linux()
        .as_ref()
        .and_then(|l| l.resources().as_ref())
        .and_then(|r| r.memory().as_ref())
        .and_then(|m| m.limit());

    let args = resolve_args(&opts.args, default_args.unwrap());

    let cpu_limit = resolve_cpu_limit(&opts.cpu_limit, default_cpu);

    let memory_limit = resolve_memory_limit(&opts.memory_limit, default_memory);

    let assigned_ip = ipam
        .allocate(&container_id)
        .await
        .map_err(|e| e.to_string())?;

    println!(
        "[IPAM] Assigned IP for container {}: {}",
        &container_id, assigned_ip
    );

    let container_workdir = CrabtainerPaths::runtime_dir().join(&container_id);

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

    // Volumes & bind mounts
    let container_init_path = merged_rootfs.join("dev/.crabtainer_init");
    let container_init_localization = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("crabtainer_init");
    fs::OpenOptions::new().create(true).write(true).open(&container_init_path).expect("[ERROR] Failed to open dev/.crabtainer_init");

    mount(
        Some(container_init_localization.to_str().unwrap()),
        &container_init_path,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_RDONLY,
        None::<&str>,
    ).expect("[ERROR] Failed to mount init program");

    println!("[HOST] Starting container {}", container_id);

    let final_opts = crate::engine::runtime::options::ContainerReady {
        layout_name: opts.layout_name.clone(),
        args: args.clone(),
        quota: Some(cpu_limit),
        memory_limit: Some(memory_limit),
        restart_policy: opts.restart_policy.clone(),
    };

    let cgroup_dir = setup_cgroups(&container_id, &final_opts)?;

    let proc_dir = merged_rootfs.join("proc");
    fs::create_dir_all(&proc_dir).ok();

    let mut stack: Vec<u8> = vec![0u8; STACK_SIZE];

    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWNET;

    let child_pid = unsafe {
        clone(
            Box::new(|| child_process(&merged_rootfs, &container_id, &final_opts)),
            &mut stack[..],
            flags,
            Some(Signal::SIGCHLD as i32),
        )
        .expect("[ERROR] Failed to spawn child.")
    };

    if let Err(e) = attach_process_to_cgroup(&cgroup_dir, child_pid) {
        eprintln!("[WARN] Failed to attach process to cgroup: {}", e);
    };

    let container_name = if let Some(name) = &opts.container_name {
        name.clone()
    } else {
        container_id.to_string()
    };
    let cwd = layout_opts
        .process()
        .as_ref()
        .and_then(|p| p.cwd().to_str())
        .expect("[ERROR] Failed to get work dir");

    let mut runtime_config = crate::engine::runtime::options::RuntimeConfig {
        container_name,
        status: ContainerStatus::Active,
        layout_name: opts.layout_name.clone(),
        ip_address: assigned_ip,
        workdir: PathBuf::from(cwd),
        pid: child_pid.as_raw(),
        boot_id: save_boot_id(),
        restart_policy: opts.restart_policy.clone(),
        is_detached: false,
        args: args.clone(),
        cpu_limit,
        memory_limit,
        rm: opts.rm,
    };

    fs::write(
        container_workdir.join("config.json"),
        serde_json::to_string_pretty(&runtime_config)
            .unwrap()
            .into_bytes(),
    )
    .unwrap();

    network_manager
        .attach_container(container_id.as_str(), child_pid.as_raw(), assigned_ip)
        .await
        .map_err(|e| e.to_string())?;

    tokio::task::spawn_blocking(move || match nix::sys::wait::waitpid(child_pid, None) {
        Ok(nix::sys::wait::WaitStatus::Exited(_, status)) => {
            println!("[INFO] Container exited with code {}", status);
            if status != 0 {
                return Err(format!(
                    "Container process failed with exit code: {}",
                    status
                ));
            }

            Ok(())
        }
        Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
            Err(format!("Container process exited with signal {:?}", sig))
        }
        Ok(status) => {
            println!("[INFO] Container proceed changed state: {:?}", status);
            Ok(())
        }
        Err(e) => Err(format!("[ERROR] Error waiting for child process: {:?}", e)),
    })
    .await
    .map_err(|e| format!("[ERROR] Error waiting for child process: {}", e))??;

    let released_ip = ipam
        .release(&container_id)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "[IPAM] Released IP for container {}: {}",
        &container_id, released_ip
    );

    if let Err(e) = fs::remove_dir(&cgroup_dir) {
        eprintln!("[WARN] Error during cgroup deletion: {}", e);
    }

    println!("[HOST] Umount overlayfs");

    if let Err(e) = umount2(&merged_rootfs, MntFlags::MNT_DETACH) {
        eprintln!("[WARN] Failed to umount overlayfs: {}", e);
    }

    runtime_config.status = ContainerStatus::Exited;

    fs::write(
        container_workdir.join("config.json"),
        serde_json::to_string_pretty(&runtime_config)
            .unwrap()
            .into_bytes(),
    )
    .unwrap();

    if opts.rm {
        fs::remove_dir_all(&container_workdir)
            .map_err(|e| format!("[WARN] Failed to delete container workdir: {}", e))?;
    }

    println!("[HOST] Container stopped");

    Ok(())
}

fn detach_child_process(
    rootfs: &Path,
    container_id: &String,
    options: &crate::engine::runtime::options::ContainerReady,
    sync_raw_fd: RawFd,
) -> isize {
    let mut buf = [0u8; 1];
    let sync_read_fd = unsafe { OwnedFd::from_raw_fd(sync_raw_fd) };

    if let Err(e) = nix::unistd::read(&sync_read_fd, &mut buf) {
        eprintln!(
            "[child] Failed sync pipe read (errno {}): {}",
            e, container_id
        );
        std::process::exit(1);
    }

    child_process(rootfs, container_id, options)
}

fn child_process(
    rootfs: &Path,
    container_id: &String,
    options: &crate::engine::runtime::options::ContainerReady,
) -> isize {
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

    fs::write(
        "/etc/resolv.conf",
        "nameserver 1.1.1.1\nnameserver 8.8.8.8\n".as_bytes(),
    )
    .expect("[ERROR] Failed to write resolv.conf file");

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

    if options.args.is_empty() {
        eprintln!("[CHILD ERROR] No command specified for container execution.");
        std::process::exit(1);
    }

    let cmd_cstring: CString = CString::new("/dev/.crabtainer_init").unwrap();
    let mut args_cstring: Vec<CString> = vec![cmd_cstring.clone()];

    args_cstring.extend(
        options
            .args
            .iter()
            .map(|arg| {
                CString::new(arg.as_str()).expect("[CHILD ERROR] Failed to convert arg to CString")
            })
            .collect::<Vec<CString>>()
    );
    
    match execvp(&cmd_cstring, &args_cstring) {
        Ok(_) => unreachable!(),
        Err(e) => {
            eprintln!("[CHILD ERROR] Failed to exec container command: {}", e);
            std::process::exit(127);
        }
    };
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
    fn generate_container_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(
                seen.insert(generate_container_id()),
                "duplicate container id"
            );
        }
        assert_eq!(seen.len(), 1000);
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

    #[test]
    fn resolve_cpu_limit_uses_cli_quota_when_given() {
        assert_eq!(resolve_cpu_limit(&Some(2.0), None), 200000);
    }

    #[test]
    fn resolve_cpu_limit_uses_layout_quota_when_no_cli() {
        assert_eq!(resolve_cpu_limit(&None, Some(50000)), 50000);
    }

    #[test]
    fn resolve_cpu_limit_defaults_when_nothing_given() {
        assert_eq!(resolve_cpu_limit(&None, None), 100000);
    }

    #[test]
    fn resolve_cpu_limit_cli_takes_precedence_over_layout() {
        assert_eq!(resolve_cpu_limit(&Some(2.0), Some(50000)), 200000);
    }

    #[test]
    fn resolve_memory_limit_uses_cli_limit_when_given() {
        assert_eq!(resolve_memory_limit(&Some(512.0), None), 512);
    }

    #[test]
    fn resolve_memory_limit_uses_layout_limit_when_no_cli() {
        assert_eq!(resolve_memory_limit(&None, Some(1024)), 1024);
    }

    #[test]
    fn resolve_memory_limit_defaults_when_nothing_given() {
        assert_eq!(resolve_memory_limit(&None, None), 1024 * 1024 * 1024);
    }

    #[test]
    fn resolve_memory_limit_cli_takes_precedence_over_layout() {
        assert_eq!(resolve_memory_limit(&Some(512.0), Some(1024)), 512);
    }
}
