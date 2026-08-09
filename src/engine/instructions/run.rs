use std::path::{Path, PathBuf};
use std::process::{Command};
use nix::unistd::{chroot, chdir, sethostname};
use nix::sched::{clone, CloneFlags};
use nix::mount::{mount, umount2, MsFlags, MntFlags};
use nix::sys::signal::Signal;
use crate::engine::paths::RustockerPaths;

pub async fn run_in_container(output_layout_name: &String, command: String) -> Result<(), String> {
    println!(" => [RUN] Running {} command in container", command);
    const STACK_SIZE: usize = 5 * 1024 * 1024;
    let rootfs = RustockerPaths::layout_store_dir().join(&output_layout_name).join("rootfs");

    let mut stack: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    let flags = CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS;

    let child_pid = unsafe {
        clone(
            Box::new(|| child_process(&rootfs, &output_layout_name, &command)),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32),
        )
            .expect(" => [RUN] Failed to spawn a child process")
    };
    eprintln!(" => [RUN] Cloned child process: {}", child_pid);

    nix::sys::wait::waitpid(child_pid, None).unwrap();

    Ok(())
}

fn child_process(rootfs: &PathBuf, layout_name: &String, command: &String) -> isize {
    let _ = sethostname(layout_name);

    let proc_target = rootfs.join("proc");
    if let Err(e) = mount(
        Some("proc"),
        &proc_target,
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    ) {
        eprintln!(" => [RUN] Failed to mount proc {}", e);
    };

    if let Err(e) = chroot(rootfs) {
        eprintln!(" => [RUN] Chroot error: {}", e);
        return 1;
    }

    if let Err(e) = chdir("/") {
        eprintln!(" => [RUN] chdir error: {}", e);
        return 1;
    }

    let status = match Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!(" => [RUN] Failed to exec /bin/sh: {}", e);
            return 1;
        }
    };

    let _ = umount2(&proc_target, MntFlags::MNT_DETACH);

    if status.success() { 0 } else { 1 }
}