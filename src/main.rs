use nix::mount::{mount, umount, MsFlags};
use nix::sched::{clone, CloneFlags};
use nix::sys::signal::Signal;
use nix::unistd::{chroot, chdir, execvp, sethostname};
use std::ffi::{CStr, CString};
use std::path::Path;

const STACK_SIZE: usize = 1024 * 1024; // 1 MB

fn main() {
    println!("[HOST] Running a isolated container...");

    let rootfs = Path::new("./alpine_rootfs");
    if (!rootfs.exists()) {
        eprintln!("[HOST] Cannot find rootfs directory");
        std::process::exit(1);
    }
    let mut stack: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    let flags = CloneFlags::CLONE_NEWUTS
                        | CloneFlags::CLONE_NEWPID
                        | CloneFlags::CLONE_NEWNS;

    let child_pid = unsafe {
        clone(
            Box::new(|| child_process(rootfs)),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32)
        )
            .expect("Failed to spawn child. Maybe you need root permissions")
    };

    nix::sys::wait::waitpid(child_pid, None).unwrap();

    println!("[HOST] Container stopped");
}

fn child_process(rootfs: &Path) -> isize {
    println!("[CONTAINER] I am on isolated namespace");

    sethostname("my-container").expect("Error on sethostname");

    chroot(rootfs);
    chdir("/");

    let none_str: Option<&str> = None;

    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::empty(),
        none_str,
    ).expect("Failed to mount proc");

    let cmd = CString::new("/bin/sh").unwrap();
    let args = [CString::new("sh").unwrap()];

    println!("[CONTAINER] Spawning /bin/sh");
    execvp(&cmd, &args).expect("Error execvp on /bin/sh");

    let _ = umount("/proc");

    0
}