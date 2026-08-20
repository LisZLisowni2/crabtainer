pub async fn init_systemd_config() -> Result<(), Box<dyn std::error::Error>> {
    let service_path = "/etc/systemd/system/rustocker-autostart.service";

    let current_exe = std::env::current_exe()?;
    let exe_path = current_exe.to_str().ok_or("Invalid executable path")?;

    let service_content = format!(
        r"[Unit]
Description=Rustocker Container Autostart Service
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