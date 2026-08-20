use std::process::{Command, ExitCode};
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::env;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("[RUSTOCKER-INIT] Usage: rustocker-init <command> [args...]");
        return ExitCode::from(1);
    }

    let child = match Command::new(&args[1])
        .args(&args[2..])
        .spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("[RUSTOCKER-INIT] Failed to exec '{}': {}", args[1], e);
            return ExitCode::from(127);
        }
    };

    let target_pid = Pid::from_raw(child.id() as i32);

    let mut signals = Signals::new(&[SIGINT, SIGTERM]).expect("Failed to register signals");
    std::thread::spawn(move || {
        for sig in signals.forever() {
            let signal = match sig {
                SIGINT => Signal::SIGINT,
                SIGTERM => Signal::SIGTERM,
                _ => continue,
            };

            let _ = kill(target_pid, signal);
        }
    });

    let mut exit_code = 0;

    loop {
        match waitpid(Pid::from_raw(-1), None) {
            Ok(WaitStatus::Exited(pid, code)) => {
                if pid == target_pid {
                    exit_code = code;
                    break;
                }
            }
            Ok(WaitStatus::Signaled(pid, signal, _)) => {
                if pid == target_pid {
                    exit_code = 128 + signal as i32;
                    break;
                }
            }
            Err(nix::errno::Errno::ECHILD) => {
                break;
            }
            Err(err) => {
                eprintln!("[RUSTOCKER-INIT] Error waitpid: '{}': {}", args[1], err);
                break;
            }
            _ => {}
        }
    }

    ExitCode::from(exit_code as u8)
}