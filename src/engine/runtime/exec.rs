use crate::engine::support::paths::RustockerPaths;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use nix::pty::OpenptyResult;
use nix::sched::CloneFlags;
use nix::unistd::{
    ForkResult, chdir, chroot, dup2_stderr, dup2_stdin, dup2_stdout, execvp, fork, setsid,
};
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};

pub struct ExecOptions {
    pub interactive: bool,
    pub tty: bool,
    pub cmd: String,
    pub args: Option<Vec<String>>,
}

pub fn exec_with_tty(
    container_pid: i32,
    container_id: String,
    opts: ExecOptions,
) -> Result<i32, Box<dyn std::error::Error>> {
    let OpenptyResult { master, slave } = nix::pty::openpty(None, None)?;

    match unsafe { fork()? } {
        ForkResult::Parent { child } => {
            drop(slave);

            enable_raw_mode()?;

            let master_read_clone = master.try_clone()?;
            let mut master_read = std::fs::File::from(master_read_clone);
            let mut master_write = std::fs::File::from(master);

            let stdout_thread = std::thread::spawn(move || {
                let mut buf = [0u8; 1024];
                let mut stdout = std::io::stdout();

                while let Ok(n) = master_read.read(&mut buf) {
                    if n == 0 {
                        break;
                    }

                    let _ = stdout.write_all(&buf[..n]);
                    let _ = stdout.flush();
                }
            });

            if opts.interactive {
                std::thread::spawn(move || {
                    let mut buf = [0u8; 1024];
                    let mut stdin = std::io::stdin();

                    while let Ok(n) = stdin.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        if master_write.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                });
            }

            let mut status = 0;
            unsafe {
                nix::libc::waitpid(child.as_raw(), &mut status, 0);
            }

            disable_raw_mode()?;

            let _ = stdout_thread.join();
            Ok(status)
        }
        ForkResult::Child => {
            drop(master);
            let _ = setsid();

            unsafe {
                nix::libc::ioctl(slave.as_raw_fd(), nix::libc::TIOCSCTTY, 0);
            }

            join_namespaces(container_pid);

            match unsafe { fork()? } {
                ForkResult::Parent { child } => {
                    let mut status = 0;

                    unsafe {
                        nix::libc::waitpid(child.as_raw(), &mut status, 0);
                    }

                    Ok(status)
                }
                ForkResult::Child => {
                    dup2_stdin(slave.as_fd())?;
                    dup2_stdout(slave.as_fd())?;
                    dup2_stderr(slave.as_fd())?;
                    drop(slave);

                    unsafe {
                        std::env::set_var("TERM", "xterm-256color");
                    }

                    let container_path = RustockerPaths::runtime_dir().join(&container_id);
                    let rootfs = container_path.join("rootfs");

                    let _ = chroot(rootfs.to_str().unwrap());
                    let _ = chdir("/");

                    let c_cmd = CString::new(opts.cmd)?;
                    let mut c_args = vec![c_cmd.clone()];
                    if let Some(args) = opts.args {
                        for arg in args {
                            c_args.push(CString::new(arg)?);
                        }
                    }

                    execvp(&c_cmd, &c_args)?;

                    unreachable!();
                }
            }
        }
    }
}

pub fn exec_with_pipes(
    container_pid: i32,
    opts: ExecOptions,
) -> Result<i32, Box<dyn std::error::Error>> {
    match unsafe { fork()? } {
        ForkResult::Parent { child } => {
            let mut status = 0;

            unsafe {
                nix::libc::waitpid(child.as_raw(), &mut status, 0);
            }

            Ok(status)
        }
        ForkResult::Child => {
            join_namespaces(container_pid);

            let c_cmd = CString::new(opts.cmd)?;
            let mut c_args = vec![c_cmd.clone()];
            if let Some(args) = opts.args {
                for arg in args {
                    c_args.push(CString::new(arg.as_str())?);
                }
            }

            let _ = execvp(&c_cmd, &c_args);

            unreachable!();
        }
    }
}

pub fn exec_in_container(
    container_pid: i32,
    container_id: String,
    opts: ExecOptions,
) -> Result<i32, Box<dyn std::error::Error>> {
    if opts.tty {
        exec_with_tty(container_pid, container_id, opts)
    } else {
        exec_with_pipes(container_pid, opts)
    }
}

fn join_namespaces(container_pid: i32) {
    let namespaces = [
        // ("ipc", CloneFlags::CLONE_NEWIPC),
        // ("uts", CloneFlags::CLONE_NEWUTS),
        // ("net", CloneFlags::CLONE_NEWNET),
        ("pid", CloneFlags::CLONE_NEWPID),
        // ("mnt", CloneFlags::CLONE_NEWNS),
    ];

    for (ns_name, flag) in namespaces {
        let ns_file = format!("/proc/{}/ns/{}", container_pid, ns_name);
        match std::fs::File::open(&ns_file) {
            Ok(file) => {
                if let Err(e) = nix::sched::setns(file.as_fd(), flag) {
                    eprintln!("[EXEC ERROR] Failed to setns for {}: {}", ns_name, e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("[EXEC ERROR] Failed to open ns file {}: {}", ns_file, e);
                std::process::exit(1);
            }
        }
    }
}
