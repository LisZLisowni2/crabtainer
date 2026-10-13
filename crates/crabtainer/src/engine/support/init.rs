//! System-level configuration helpers.
//!
//! Writes the host configuration crabtainer needs to operate: a systemd
//! autostart unit pointing at the current executable, and sysctl rules that
//! enable local-network routing for the container bridge.

/// Writes the systemd oneshot unit `crabtainer-autostart.service` that runs
/// `crabtainer system autostart` after the network is online.
///
/// The unit's `ExecStart` points at the currently running executable, so it
/// stays valid across install locations.
///
/// # Errors
///
/// Returns an error if the executable path is invalid or the unit file cannot
/// be written to `/etc/systemd/system/`.
pub async fn init_systemd_config() -> Result<(), Box<dyn std::error::Error>> {
    let service_path = "/etc/systemd/system/crabtainer-autostart.service";

    let current_exe = std::env::current_exe()?;
    let exe_path = current_exe.to_str().ok_or("Invalid executable path")?;

    let service_content = format!(
        r"[Unit]
Description=Crabtainer Container Autostart Service
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart={} system autostart
KillMode=mixed
TimeoutStopSec=10s

[Install]
WantedBy=multi-user.target
        ",
        exe_path
    );

    tokio::fs::write(&service_path, service_content.as_bytes()).await?;

    Ok(())
}

/// Enables `net.ipv4.conf.all.route_localnet` via a sysctl drop-in and
/// applies it immediately.
///
/// The rule is written to `/etc/sysctl.d/99-crabtainer.conf` and applied with
/// `sysctl -p`. Failures to apply the rule are logged but not fatal.
pub async fn enable_route_localnet() {
    let file_path = "/etc/sysctl.d/99-crabtainer.conf";
    let content = "net.ipv4.conf.all.route_localnet = 1";

    tokio::fs::create_dir_all("/etc/sysctl.d")
        .await
        .expect("Failed to create sysctl.d");

    tokio::fs::write(file_path, content.as_bytes())
        .await
        .expect("Failed to write sysctl rule");

    match tokio::process::Command::new("sysctl")
        .arg("-p")
        .arg("/etc/sysctl.d/99-crabtainer.conf")
        .status()
        .await
    {
        Ok(_) => println!("Successfully edited sysctl rule"),
        Err(e) => eprintln!("Failed to edit sysctl rule: {}", e),
    };
}

/// Initializes all crabtainer system configuration.
///
/// Always enables the local-network routing sysctl, and additionally installs
/// the systemd autostart unit when the host's `/sbin/init` reports it is
/// running systemd.
///
/// # Errors
///
/// Returns an error if `stat` can't be run or its output can't be decoded.
pub async fn init_crabtainer_system_config() -> Result<(), String> {
    enable_route_localnet().await;

    let result = tokio::process::Command::new("stat")
        .arg("/sbin/init")
        .output()
        .await
        .expect("Failed to run command stat");

    let output_vec = result.stdout.to_vec();

    let output_str = str::from_utf8(&output_vec).expect("Failed to convert from Vec<u8> to &str");

    let splited_output: Vec<&str> = output_str.split('\n').collect();

    let file_output = splited_output[0];

    if file_output.contains("systemd") {
        init_systemd_config()
            .await
            .expect("Failed to setup autostart");
    }

    Ok(())
}
