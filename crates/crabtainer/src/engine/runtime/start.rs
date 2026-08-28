use crate::engine::runtime::container::{run_container, spawn_detach_container};
use crate::engine::runtime::options::{ContainerOptions, ContainerStatus, RuntimeConfig};
use crate::engine::support::paths::CrabtainerPaths;

async fn retrieve_config(
    container_id: &String,
) -> Result<RuntimeConfig, Box<dyn std::error::Error>> {
    let config_path = CrabtainerPaths::runtime_dir().join(&container_id).join("config.json");
    let content = std::fs::read_to_string(config_path)?;
    let config = match serde_json::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("[ERROR] Failed to load runtime config: {}", e);
            std::process::exit(1);
        }
    };

    Ok(config)
}

pub async fn restart_container(
    container_id: String
) -> Result<(), Box<dyn std::error::Error>> {
    let config = retrieve_config(&container_id).await?;

    if config.status != ContainerStatus::Active {
        eprintln!("[ERROR] You can restart only active container");
        return Ok(());
    }

    let pid = config.pid;

    crate::engine::runtime::stop::stop_container(pid).await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    start_container(container_id).await?;

    Ok(())
}

pub async fn start_container(
    container_id: String
) -> Result<(), Box<dyn std::error::Error>>  {
    let config = retrieve_config(&container_id).await?;

    if config.status == ContainerStatus::Active {
        eprintln!("[ERROR] You can manually start only stopped containers");
        return Ok(());
    }

    let opts = ContainerOptions {
        container_name: Some(config.container_name),
        args: config.args,
        cpu_limit: Some(config.cpu_limit as f64),
        memory_limit: Some(config.memory_limit as f64),
        rm: config.rm,
        restart_policy: config.restart_policy,
        layout_name: config.layout_name,
    };
    
    if config.is_detached {
        spawn_detach_container(opts, container_id).await?;
    } else {
        run_container(opts, container_id).await?;
    }
    
    Ok(())
}