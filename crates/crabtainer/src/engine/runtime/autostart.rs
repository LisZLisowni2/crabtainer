use crate::engine::runtime::container::spawn_detach_container;
use crate::engine::runtime::network::Ipam;
use crate::engine::runtime::options::{ContainerOptions, ContainerStatus, RestartPolicy, RuntimeConfig};
use crate::engine::support::paths::CrabtainerPaths;

pub async fn autostart_detached() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_dir = CrabtainerPaths::runtime_dir();

    if !runtime_dir.exists() {
        return Ok(());
    }

    let ipam = Ipam::new(
        "172.19.0.1/16",
        CrabtainerPaths::base_dir().join("config.json"),
    )?;

    for entry in runtime_dir.read_dir()? {
        let path = entry?.path();
        let config_path = path.join("config.json");

        let container_id = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let _ = ipam.release(&container_id).await;

        if !config_path.exists() {
            continue;
        }

        let runtime_config_content = std::fs::read_to_string(config_path)?;
        let config: RuntimeConfig = match serde_json::from_str(&runtime_config_content) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let should_autostart = config.is_detached && match config.restart_policy {
            RestartPolicy::Never => false,
            RestartPolicy::Always => config.status != ContainerStatus::Stopped,
            RestartPolicy::UnlessStopped => config.status != ContainerStatus::Stopped,
            RestartPolicy::OnFailure => config.status == ContainerStatus::Error
        };

        if should_autostart {
            let opts: ContainerOptions = ContainerOptions {
                layout_name: config.layout_name,
                container_name: Some(config.container_name),
                restart_policy: config.restart_policy,
                args: config.args,
                cpu_limit: Some(config.cpu_limit as f64),
                memory_limit: Some(config.memory_limit as f64),
                rm: config.rm,
            };

            if let Err(e) = spawn_detach_container(opts, container_id).await {
                eprintln!("[AUTOSTART] Failed to restart detached container: {}", e);
            }
        }
    }

    Ok(())
}