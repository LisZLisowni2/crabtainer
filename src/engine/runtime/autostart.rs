use crate::engine::runtime::options::{RestartPolicy, RuntimeConfig};
use crate::engine::support::paths::RustockerPaths;

pub async fn autostart_detached() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_dir = RustockerPaths::runtime_dir();

    if !runtime_dir.exists() {
        return Ok(());
    }

    for entry in runtime_dir.read_dir()? {
        let path = entry?.path();
        let config_path = path.join("config.json");

        if !config_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(config_path)?;
        let config: RuntimeConfig = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if config.is_detached && config.restart_policy == RestartPolicy::Always {
            let container_id = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();

            if let Err(e) = restart_detached_container(container_id) {
                eprintln!("[AUTOSTART] Failed to restart detached container: {}", e);
            }
        }
    }
}

async fn restart_detached_container(container_id: String) -> Result<(), Box<dyn std::error::Error>> {
    
}