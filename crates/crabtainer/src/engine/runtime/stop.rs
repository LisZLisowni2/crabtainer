pub async fn stop_container(
    container_pid: i32,
) -> Result<(), String> {
    let pid = nix::unistd::Pid::from_raw(container_pid);

    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM)
        .expect("[ERROR] Failed to kill container");

    println!("[HOST] Container stopped");

    Ok(())
}
