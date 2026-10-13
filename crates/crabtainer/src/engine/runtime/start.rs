//! Manually starting and restarting existing containers.
//!
//! While [`container`](crate::engine::runtime::container) focuses on the
//! initial spawn, this module re-uses the stored [`RuntimeConfig`] to bring a
//! previously stopped container back up, either foregrounded or detached,
//! matching its original options.

use crate::engine::runtime::container::{run_container, spawn_detach_container};
use crate::engine::runtime::options::{ContainerOptions, ContainerStatus, RuntimeConfig};
use crate::engine::support::paths::CrabtainerPaths;

/// Loads the [`RuntimeConfig`] persisted at `containers/<id>/config.json` and
/// returns it.
///
/// Terminates the process with an error message if the config cannot be read
/// or parsed.
async fn retrieve_config(
    container_id: &String,
) -> Result<RuntimeConfig, Box<dyn std::error::Error>> {
    let config_path = CrabtainerPaths::runtime_dir()
        .join(container_id)
        .join("config.json");
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

/// Restarts an **active** container by stopping it and starting it again
/// from its stored config.
///
/// Only containers whose status is [`Active`](ContainerStatus::Active) can be
/// restarted; anything else is reported as an error and left untouched.
///
/// # Arguments
///
/// * `container_id` - Directory name of the container under the runtime dir.
///
/// # Errors
///
/// Fails if the config cannot be loaded or either the stop or start step
/// fails.
pub async fn restart_container(container_id: String) -> Result<(), Box<dyn std::error::Error>> {
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

/// Starts a previously stopped container from its stored [`RuntimeConfig`].
///
/// The container is re-spawned with the exact options it was created with
/// (layout, args, limits, ports, volumes, restart policy). Detached
/// containers are spawned in the background; foreground containers block
/// until they exit. Active containers are rejected.
///
/// # Arguments
///
/// * `container_id` - Directory name of the container under the runtime dir.
///
/// # Errors
///
/// Fails if the config cannot be loaded or the underlying spawn fails.
pub async fn start_container(container_id: String) -> Result<(), Box<dyn std::error::Error>> {
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
        ports: config.ports,
        envs: config.envs,
        volumes: config.volumes,
    };

    if config.is_detached {
        spawn_detach_container(opts, container_id).await?;
    } else {
        run_container(opts, container_id).await?;
    }

    Ok(())
}
