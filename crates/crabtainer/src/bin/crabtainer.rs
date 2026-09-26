use clap::{Parser, Subcommand};
use crabtainer::engine::build::builder::build_layout;
use crabtainer::engine::runtime::container::{run_container, spawn_detach_container};
use crabtainer::engine::runtime::exec::ExecOptions;
use crabtainer::engine::runtime::options::{
    ContainerOptions, ContainerStatus, RestartPolicy, RuntimeConfig,
};
use crabtainer::engine::runtime::stop::stop_container;
use crabtainer::engine::support::paths::CrabtainerPaths;
use getch_rs::Key;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;
use walkdir::WalkDir;

/// Crabtainer - A lightweight daemonless container engine built from scratch in Rust
///
/// Crabtainer allows running isolated processes using Linux namespaces, cgroups,
/// and custom overlay filesystems without relying on a background daemon.
#[derive(Parser)]
#[command(name = "crabtainer")]
#[command(
    about = "Crabtainer - A lightweight daemonless container engine built from scratch in Rust "
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create and start a container from a specific layout
    Run {
        /// The target layout tag to be executed in container
        layout: String,

        /// Run container in background and print container ID
        #[arg(short, long, default_value_t = false)]
        detach: bool,

        /// Automaticallly remove container dir after exit
        #[arg(long, default_value_t = false)]
        rm: bool,

        /// Assign a custom name to the container
        #[arg(short, long)]
        name: Option<String>,

        /// Set maximum CPU limit (e.g. 1.5 for 1.5 CPU Cores)
        #[arg(short = 'C', long)]
        cpu_limit: Option<f64>,

        /// Forward a container port to the host (e.g. -p 8000:80)
        #[arg(short, long, action = clap::ArgAction::Append)]
        port: Vec<String>,

        /// Set environment variables inside the container (e.g. -e KEY=VALUE)
        #[arg(short, long, action = clap::ArgAction::Append)]
        env: Vec<String>,

        /// Bind mount a volume or host path into the container
        #[arg(short, long, action = clap::ArgAction::Append)]
        volume: Vec<String>,

        /// Set maximum memoryt limit specified in MB
        #[arg(short = 'M', long)]
        memory_limit: Option<f64>,

        /// Specify restart policy to apply when a container exits
        #[arg(short, long, default_value_t, value_enum)]
        restart: RestartPolicy,

        /// Specify an explicit command to execute inside the container
        #[arg(short, long)]
        command: Option<String>,

        /// Additional arguments passed to command
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            requires = "command"
        )]
        args: Option<Vec<String>>,
    },

    /// Build a new container image layout using a spec file (Crabtainerfile)
    Build {
        /// Path to the build instructions file
        #[arg(short, long, default_value = "Crabtainerfile")]
        file: String,

        /// Tag to assign to the built image layout
        #[arg(short, long)]
        tag: String,
    },

    /// List active and managed containers
    Ps,

    /// Start stopped container
    Start { name: String },

    /// Restart active or error container
    Restart { name: String },
    /// Stop active container
    Stop { name: String },
    /// Remove stopped container
    Rm { name: String },
    /// Execute a command inside the container
    Exec {
        /// Enter the interactive mode inside the container
        #[arg(short, long, default_value_t = false)]
        interactive: bool,

        /// Enter the terminal inside the container
        #[arg(short, long, default_value_t = false)]
        tty: bool,

        /// Target container (specific name)
        name: String,

        /// Command to be executed inside container
        cmd: String,

        /// Optional commands arguments passed to command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, requires = "cmd")]
        args: Option<Vec<String>>,
    },

    /// Manage downloaded images or pull one
    Image {
        #[command(subcommand)]
        action: ImageActions,
    },

    /// Manage available layouts
    Layout {
        #[command(subcommand)]
        action: LayoutActions,
    },

    /// Manage system-related crabtainer's things
    System {
        #[command(subcommand)]
        action: SystemActions,
    },

    /// Manage persistent data storage volumes
    Volume {
        #[command(subcommand)]
        action: VolumeActions,
    },
}

#[derive(Subcommand)]
enum VolumeActions {
    /// Remove all unused local volumes
    Prune,
    /// Create a new named volume
    Create,
    /// List all volumes
    Ls,
    /// Remove a specific volume
    Remove,
}

#[derive(Subcommand)]
enum SystemActions {
    Prune,
    /// Initialize mandatory crabtainer's configuration across system  
    Init,
    /// Enable autostart via systemd (in future other init systems)
    Autostart,
}

#[derive(Subcommand)]
enum ImageActions {
    /// List all images
    Ps,

    /// Remove a specific image
    Rm { name: String },

    /// Download a image from public registry
    Pull {
        /// Image ref to be downloaded
        image: String,
        /// Alias of downloaded image
        alias: String,
        /// Delete existing image
        #[arg(short, long, default_value_t = false)]
        overriding: bool,
    },

    /// Display details about image
    Inspect { name: String },
}

#[derive(Subcommand)]
enum LayoutActions {
    /// List all layouts
    Ps,

    /// Remove specific layout
    Rm {
        /// Specific tag name of layout
        tag: String,
    },

    /// Display details about layout
    Inspect {
        /// Specific tag name of layout
        tag: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if !is_root::is_root() {
        eprintln!("You have to run this command with sudo privileges");
        return;
    }

    match cli.command {
        Commands::Run {
            layout,
            command,
            args,
            cpu_limit,
            memory_limit,
            restart,
            rm,
            name,
            detach,
            port,
            env,
            volume,
        } => {
            let mut final_command: Vec<String> = vec![];

            if let Some(cmd) = command {
                final_command.push(cmd);
            }

            if let Some(arguments) = args {
                final_command.extend(arguments);
            }

            let options = ContainerOptions {
                layout_name: layout,
                args: final_command,
                cpu_limit,
                memory_limit,
                container_name: name,
                rm,
                restart_policy: restart,
                ports: port,
                envs: env,
                volumes: volume,
            };

            let container_id = crabtainer::engine::runtime::container::generate_container_id();

            // TODO: Handle bad commands (empty or invalid ones)
            if !detach {
                run_container(options, container_id).await.unwrap();
            } else {
                spawn_detach_container(options, container_id).await.unwrap();
            }
        }
        Commands::Build { file, tag } => {
            let canonicalized = std::fs::canonicalize(file)
                .expect("Failed to canonicalize crabtainerfile path")
                .into_string()
                .expect("Failed to convert to String");
            build_layout(canonicalized, tag).await.unwrap();
        }
        Commands::Image { action } => match action {
            ImageActions::Inspect { name } => {
                let store = CrabtainerPaths::image_store_dir();
                let config_path = store.join(name).join("config.json");
                if let Ok(content) = std::fs::read_to_string(config_path)
                    && let Ok(image_config) =
                        serde_json::from_str::<oci_client::config::ConfigFile>(content.as_str())
                    && let Ok(string_pretty) = serde_json::to_string_pretty(&image_config)
                {
                    println!("{}", string_pretty);
                }
            }
            ImageActions::Rm { name } => {
                let store = CrabtainerPaths::image_store_dir();
                if name != "." {
                    match std::fs::remove_dir_all(store.join(&name)) {
                        Ok(_) => println!("{}", name),
                        Err(e) => eprintln!("[ERROR] Failed to remove image: {}", e),
                    }
                } else {
                    println!("Are you sure to delete all images? [y/n]");
                    let g = getch_rs::Getch::new();

                    loop {
                        match g.getch() {
                            Ok(Key::Char('y')) => {
                                for entry in store.read_dir().unwrap().flatten() {
                                    let path = entry.path();
                                    let name = path.file_name().unwrap().to_str().unwrap();

                                    match std::fs::remove_dir_all(&path) {
                                        Ok(_) => {}
                                        Err(e) => eprintln!(
                                            "[WARN] Failed to remove image {}: {}",
                                            name, e
                                        ),
                                    }
                                }
                                break;
                            }
                            Ok(Key::Char('n')) => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            ImageActions::Pull {
                image,
                alias,
                overriding,
            } => {
                crabtainer::engine::build::instructions::download::download_image_if_missing(
                    image.as_str(),
                    alias.as_str(),
                    overriding,
                )
                .await
                .unwrap();
            }
            ImageActions::Ps => {
                let store = CrabtainerPaths::image_store_dir();
                println!("{:<20} {:<15}", "ALIAS", "SIZE");
                println!("{}", "-".repeat(38));
                std::fs::read_dir(&store).ok();

                if let Ok(entries) = std::fs::read_dir(store) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = path
                            .file_stem()
                            .unwrap()
                            .to_string_lossy()
                            .replace(".tar.gz", "");

                        let size = if path.is_dir() {
                            WalkDir::new(path)
                                .into_iter()
                                .filter_map(|e| e.ok())
                                .filter_map(|e| e.metadata().ok())
                                .filter(|m| m.is_file())
                                .map(|m| m.len())
                                .sum()
                        } else {
                            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
                        };

                        println!("{:<20} {:<15} MB", name, size / 1024 / 1024);
                    }
                }
            }
        },
        Commands::Volume { action } => match action {
            VolumeActions::Prune => {}
            VolumeActions::Create => {}
            VolumeActions::Ls => {}
            VolumeActions::Remove => {}
        },
        Commands::Layout { action } => match action {
            LayoutActions::Rm { tag } => {
                let store = CrabtainerPaths::layout_store_dir();
                let runtime_dir = CrabtainerPaths::runtime_dir();
                if tag != "." {
                    let mut is_found = false;

                    for entry in runtime_dir.read_dir().unwrap().flatten() {
                        let path = entry.path();
                        let config_path = path.join("config.json");

                        if let Ok(content) = std::fs::read_to_string(&config_path)
                            && let Ok(config) = serde_json::from_str::<RuntimeConfig>(&content)
                            && config.layout_name == tag
                        {
                            is_found = true;
                            break;
                        }
                    }

                    if is_found {
                        eprintln!(
                            "[ERROR] One of containers use this layout, delete it before deleting layout."
                        );
                        return;
                    }

                    if let Err(e) = std::fs::remove_dir_all(store.join(&tag)) {
                        eprintln!("[WARN] Failed to remove layout {}: {}", tag, e);
                    };
                } else {
                    println!("Are you sure to delete all unused layouts? [y/n]");
                    let g = getch_rs::Getch::new();

                    loop {
                        match g.getch() {
                            Ok(Key::Char('y')) => {
                                let mut container_layout_hashset: HashSet<String> = HashSet::new();

                                for entry in runtime_dir.read_dir().unwrap().flatten() {
                                    let path = entry.path();
                                    let config_path = path.join("config.json");

                                    if let Ok(content) = std::fs::read_to_string(&config_path)
                                        && let Ok(config) =
                                            serde_json::from_str::<RuntimeConfig>(&content)
                                    {
                                        container_layout_hashset.insert(config.layout_name);
                                    }
                                }

                                for entry in store.read_dir().unwrap().flatten() {
                                    let path = entry.path();
                                    let name = path.file_name().unwrap().to_str().unwrap();

                                    if !container_layout_hashset.contains(name)
                                        && let Err(e) = std::fs::remove_dir_all(store.join(&tag))
                                    {
                                        eprintln!("[WARN] Failed to remove image {}: {}", tag, e);
                                    };
                                }
                            }
                            Ok(Key::Char('n')) => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            LayoutActions::Inspect { tag } => {
                let store = CrabtainerPaths::layout_store_dir();
                let config_path = store.join(tag).join("config.json");

                if let Ok(content) = std::fs::read_to_string(config_path)
                    && let Ok(image_config) =
                        serde_json::from_str::<oci_spec::runtime::Spec>(content.as_str())
                    && let Ok(string_pretty) = serde_json::to_string_pretty(&image_config)
                {
                    println!("{}", string_pretty);
                }
            }
            LayoutActions::Ps => {
                let store = CrabtainerPaths::layout_store_dir();
                println!("{:<20} {:<15}", "LAYOUT TAG", "SIZE");
                println!("{}", "-".repeat(38));

                if let Ok(entries) = std::fs::read_dir(store) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = path
                            .file_stem()
                            .unwrap()
                            .to_string_lossy()
                            .replace(".tar.gz", "");

                        let size = if path.is_dir() {
                            WalkDir::new(path)
                                .into_iter()
                                .filter_map(|e| e.ok())
                                .filter_map(|e| e.metadata().ok())
                                .filter(|m| m.is_file())
                                .map(|m| m.len())
                                .sum()
                        } else {
                            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
                        };

                        println!("{:<20} {:<15} MB", name, size / 1024 / 1024);
                    }
                }
            }
        },
        Commands::Ps => {
            crabtainer::engine::runtime::refresh::refresh_container_states()
                .await
                .expect("[ERROR] Failed to refresh container states");
            let container_dir = CrabtainerPaths::runtime_dir();
            println!(
                "{:<15} {:<20} {:<20} {:<15}",
                "ID", "NAME", "LAYOUT", "STATUS"
            );
            println!("{}", "-".repeat(80));
            if let Ok(entries) = std::fs::read_dir(container_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let id = path.file_stem().unwrap().to_str().unwrap();

                    let runtime_config_path = path.join("config.json");
                    if let Ok(content) = std::fs::read_to_string(runtime_config_path) {
                        match serde_json::from_str::<RuntimeConfig>(content.as_str()) {
                            Err(_) => eprintln!("[WARN] Failed to retrieve data for {}", id),
                            Ok(data) => println!(
                                "{:<15} {:<20} {:<20} {:<15}",
                                id, data.container_name, data.layout_name, data.status
                            ),
                        }
                    } else {
                        eprintln!("[WARN] Failed to retrieve data for {}", id);
                    }
                }
            }
        }
        Commands::Stop { name } => {
            crabtainer::engine::runtime::refresh::refresh_container_states()
                .await
                .expect("[ERROR] Failed to refresh container states");

            let id = match search_id_by_name(name).await {
                Some(id) => id,
                None => {
                    eprintln!("[WARN] Failed to find id for provided name");
                    return;
                }
            };

            let runtime_dir = CrabtainerPaths::runtime_dir().join(&id);
            let target_pid = find_pid(&id, &runtime_dir);

            let config_str = std::fs::read_to_string(runtime_dir.join("config.json"))
                .expect("[ERROR] Failed to read config");
            let config = serde_json::from_str::<RuntimeConfig>(&config_str)
                .expect("[ERROR] Failed to parse config");

            if config.status == ContainerStatus::Active {
                stop_container(target_pid)
                    .await
                    .expect("[ERROR] Failed to stop container");
            }
        }
        Commands::Rm { name } => {
            crabtainer::engine::runtime::refresh::refresh_container_states()
                .await
                .expect("[ERROR] Failed to refresh container states");

            if name != "." {
                let id = match search_id_by_name(name).await {
                    Some(id) => id,
                    None => {
                        eprintln!("[ERROR] Failed to find id for provided name");
                        return;
                    }
                };
                handle_deletion_of_container(id).await;
            } else {
                println!("Are you sure to delete all stopped and exited containers? [y/n]");
                let g = getch_rs::Getch::new();

                loop {
                    match g.getch() {
                        Ok(Key::Char('n')) => {
                            break;
                        }
                        Ok(Key::Char('y')) => {
                            if let Ok(dirs) = std::fs::read_dir(CrabtainerPaths::runtime_dir()) {
                                for entry in dirs.flatten() {
                                    let path = entry.path();
                                    let name =
                                        path.file_name().unwrap().to_str().unwrap().to_string();

                                    handle_deletion_of_container(name).await;
                                }
                            }
                            break;
                        }
                        Err(e) => eprintln!("[WARN] Failed to get ch: {}", e),
                        _ => {}
                    }
                }
            }
        }
        Commands::Exec {
            name,
            interactive,
            tty,
            cmd,
            args,
        } => {
            let id = match search_id_by_name(name).await {
                Some(id) => id,
                None => {
                    eprintln!("[WARN] Failed to find id for provided name");
                    return;
                }
            };

            let runtime_dir = CrabtainerPaths::runtime_dir().join(&id);
            let target_pid = find_pid(&id, runtime_dir);

            let opts = ExecOptions {
                interactive,
                tty,
                cmd,
                args,
            };

            handle_exec(target_pid, id, opts)
                .await
                .expect("[ERROR] Failed to execute handle_exec");
        }
        Commands::Start { name } => {
            crabtainer::engine::runtime::refresh::refresh_container_states()
                .await
                .expect("[ERROR] Failed to refresh container states");

            let id = match search_id_by_name(name).await {
                Some(id) => id,
                None => {
                    eprintln!("[WARN] Failed to find id for provided name");
                    return;
                }
            };
            crabtainer::engine::runtime::start::start_container(id)
                .await
                .unwrap();
        }
        Commands::Restart { name } => {
            crabtainer::engine::runtime::refresh::refresh_container_states()
                .await
                .expect("[ERROR] Failed to refresh container states");

            let id = match search_id_by_name(name).await {
                Some(id) => id,
                None => {
                    eprintln!("[WARN] Failed to find id for provided name");
                    return;
                }
            };
            crabtainer::engine::runtime::start::restart_container(id)
                .await
                .unwrap();
        }
        Commands::System { action } => match action {
            SystemActions::Prune => {}
            SystemActions::Init => {
                crabtainer::engine::support::init::init_crabtainer_system_config()
                    .await
                    .unwrap();
            }
            SystemActions::Autostart => {
                crabtainer::engine::runtime::autostart::autostart_detached()
                    .await
                    .unwrap();
            }
        },
    }
}

async fn handle_deletion_of_container(id: String) {
    let runtime_dir = CrabtainerPaths::runtime_dir().join(&id);

    if let Ok(content) = std::fs::read_to_string(runtime_dir.join("config.json"))
        && let Ok(config) = serde_json::from_str::<RuntimeConfig>(&content)
    {
        if config.status == ContainerStatus::Active {
            eprintln!("[ERROR] Active container cannot be deleted. Stop it first");
            return;
        }

        if let Err(e) = std::fs::remove_dir_all(&runtime_dir) {
            eprintln!("[WARN] Failed to remove container dir: {}", e);
        } else {
            println!("{}", id);
        }
    }
}

async fn search_id_by_name(name: String) -> Option<String> {
    let runtime_dir = CrabtainerPaths::runtime_dir();
    let entries = std::fs::read_dir(&runtime_dir).expect("[ERROR] Failed to read dir");

    for entry in entries.flatten() {
        let path = entry.path();

        let config_content = std::fs::read_to_string(path.join("config.json"))
            .expect("[ERROR] Failed to read config");
        let config: RuntimeConfig = match serde_json::from_str(config_content.as_str()) {
            Ok(cfg) => cfg,
            Err(_) => continue,
        };

        if config.container_name == name {
            return Some(path.file_name().unwrap().to_str().unwrap().to_string());
        }
    }

    None
}

fn find_pid<'a, P>(id: &String, container_dir: P) -> i32
where
    P: Into<Cow<'a, Path>>,
{
    let mut target_pid: i32 = 0;

    if let Ok(content) = std::fs::read_to_string(container_dir.into().join("config.json")) {
        match serde_json::from_str::<RuntimeConfig>(content.as_str()) {
            Ok(config) => {
                target_pid = config.pid;
            }
            Err(_) => eprintln!("[WARN] Failed to retrieve data for {}", id),
        }
    }

    target_pid
}

pub async fn handle_exec(
    container_pid: i32,
    container_id: String,
    opts: ExecOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio::task::spawn_blocking(move || {
        crabtainer::engine::runtime::exec::exec_in_container(container_pid, container_id, opts)
            .expect("[ERROR] Failed to exec in container");
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_run(args: &[&str]) -> Commands {
        let mut full = vec!["crabtainer", "run"];
        full.extend_from_slice(args);
        println!("{:?}", full);
        Cli::try_parse_from(full).unwrap().command
    }

    #[test]
    fn run_parses_all_parameters() {
        match parse_run(&[
            "-n",
            "MyContainer",
            "--rm",
            "-r",
            "always",
            "-d",
            "-p",
            "1920:1920",
            "-p",
            "2952:2952",
            "-e FALL=2",
            "-e HAND=4",
            "-v koza:/app/",
            "-v $(pwd)/data:/app/",
            "-C",
            "1.5",
            "-M",
            "2048",
            "my-layout",
            "-c",
            "/bin/sh",
        ]) {
            Commands::Run {
                layout,
                cpu_limit,
                memory_limit,
                command,
                args,
                name,
                detach,
                rm,
                restart,
                port,
                env,
                volume,
            } => {
                assert_eq!(layout, "my-layout");
                assert_eq!(cpu_limit, Some(1.5));
                assert_eq!(memory_limit, Some(2048.0));
                assert_eq!(command, Some("/bin/sh".to_string()));
                assert_eq!(args, None);
                assert!(detach);
                assert_eq!(name, Some("MyContainer".to_string()));
                assert!(rm);
                assert_eq!(restart, RestartPolicy::Always);
                assert_eq!(port, vec!["1920:1920", "2952:2952"]);
                assert_eq!(
                    env.iter()
                        .map(|e| e.strip_prefix(" ").expect("Failed to strip prefix"))
                        .collect::<Vec<&str>>(),
                    vec!["FALL=2", "HAND=4"]
                );
                assert_eq!(
                    volume
                        .iter()
                        .map(|e| e.strip_prefix(" ").expect("Failed to strip prefix"))
                        .collect::<Vec<&str>>(),
                    vec!["koza:/app/", "$(pwd)/data:/app/"]
                );
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn run_limits_default_to_none() {
        match parse_run(&["my-layout"]) {
            Commands::Run {
                layout,
                cpu_limit,
                memory_limit,
                command,
                name,
                args,
                detach,
                rm,
                restart,
                port,
                env,
                volume,
            } => {
                assert_eq!(layout, "my-layout");
                assert_eq!(cpu_limit, None);
                assert_eq!(memory_limit, None);
                assert_eq!(command, None);
                assert_eq!(args, None);
                assert!(!detach);
                assert_eq!(name, None);
                assert!(!rm);
                assert_eq!(restart, RestartPolicy::Never);
                assert_eq!(port, Vec::<String>::new());
                assert_eq!(env, Vec::<String>::new());
                assert_eq!(volume, Vec::<String>::new());
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn run_collects_trailing_args() {
        match parse_run(&["my-layout", "-c", "/bin/sh", "echo", "hi"]) {
            Commands::Run { args, command, .. } => {
                assert_eq!(command, Some("/bin/sh".to_string()));
                assert_eq!(args, Some(vec!["echo".to_string(), "hi".to_string()]));
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn run_requires_command_when_args_given() {
        let result = Cli::try_parse_from(["crabtainer", "run", "my-layout", "some-arg"]);
        assert!(
            result.is_err(),
            "expected error when args present without command"
        );
    }

    #[test]
    fn run_accepts_long_flag_forms() {
        match parse_run(&[
            "my-layout",
            "--cpu-limit",
            "0.5",
            "--memory-limit",
            "1024",
            "--command",
            "true",
        ]) {
            Commands::Run {
                cpu_limit,
                memory_limit,
                command,
                ..
            } => {
                assert_eq!(cpu_limit, Some(0.5));
                assert_eq!(memory_limit, Some(1024.0));
                assert_eq!(command, Some("true".to_string()));
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn build_parses_file_and_tag() {
        let cli = Cli::try_parse_from([
            "crabtainer",
            "build",
            "-f",
            "Crabtainerfile.dev",
            "-t",
            "my-image",
        ])
        .unwrap();
        match cli.command {
            Commands::Build { file, tag } => {
                assert_eq!(file, "Crabtainerfile.dev");
                assert_eq!(tag, "my-image");
            }
            _ => panic!("expected Build command"),
        }
    }

    #[test]
    fn build_uses_default_file() {
        let cli = Cli::try_parse_from(["crabtainer", "build", "-t", "my-image"]).unwrap();
        match cli.command {
            Commands::Build { file, .. } => assert_eq!(file, "Crabtainerfile"),
            _ => panic!("expected Build command"),
        }
    }
}
