//! Stopping a running container.
//!
//! The stop path is intentionally simple: the container's root process is
//! terminated with `SIGTERM` and the container's init (`crabtainer_init`)
//! takes care of propagating it to the containerized command.

/// Sends `SIGTERM` to the container's root process, politely asking it to
/// shut down.
///
/// # Arguments
///
/// * `container_pid` - Host-side PID of the container's root process (as
///   stored in the container's `config.json`).
///
/// # Errors
///
/// The function panics (via `expect`) if the signal cannot be delivered;
/// it only returns `Err` if the container PID is invalid.
pub async fn stop_container(container_pid: i32) -> Result<(), String> {
    let pid = nix::unistd::Pid::from_raw(container_pid);

    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM)
        .expect("[ERROR] Failed to kill container");

    println!("[HOST] Container stopped");

    Ok(())
}
