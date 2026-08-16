use std::borrow::Cow;
use std::io::Error;
use clap::{Parser, Subcommand};
use nix::sched::CloneFlags;
use rustocker::engine::build::builder::build_layout;
use rustocker::engine::runtime::container::{run_container, spawn_detach_container};
use rustocker::engine::runtime::options::{ContainerOptions, ContainerStatus, RuntimeConfig};
use rustocker::engine::support::paths::RustockerPaths;
use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
use std::ptr::null;
use getch_rs::Key;
use walkdir::WalkDir;
use rustocker::engine::runtime::exec::ExecOptions;
use rustocker::engine::runtime::network::Ipam;
use rustocker::engine::runtime::stop::stop_container;

#[derive(Parser)]
#[command(name = "rustocker")]
#[command(about = "Rustocker - A lightweight container runtime built from scratch in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        layout: String,

        #[arg(short, long, default_value_t = false)]
        detach: bool,

        #[arg(long, default_value_t = false)]
        rm: bool,

        #[arg(short, long, default_value = "")]
        name: Option<String>,

        #[arg(short = 'C', long)]
        cpu_limit: Option<f64>,

        #[arg(short = 'M', long)]
        memory_limit: Option<f64>,

        #[arg(short, long)]
        command: Option<String>,

        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            requires = "command"
        )]
        args: Option<Vec<String>>,
    },
    Build {
        #[arg(short, long, default_value = "Rustockerfile")]
        file: String,

        #[arg(short, long)]
        tag: String,
    },
    Ps,
    Stop {
        id: String,
    },
    Rm {
        id: String,
    },
    Exec {
        #[arg(short, long, default_value_t = false)]
        interactive: bool,

        #[arg(short, long, default_value_t = false)]
        tty: bool,

        id: String,

        cmd: String,

        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            requires = "cmd"
        )]
        args: Option<Vec<String>>,
    },
    Refresh,
    Images,
    Layouts,
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
            rm,
            name,
            detach,
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
            };

            let container_id = rustocker::engine::runtime::container::generate_container_id();

            // TODO: Handle bad commands (empty or invalid ones)
            if !detach {
                run_container(options, container_id).await.unwrap();
            } else {
                spawn_detach_container(options, container_id).await.unwrap();
            }
        }
        Commands::Build { file, tag } => {
            build_layout(file, tag).await.unwrap();
        }
        Commands::Images => {
            let store = rustocker::engine::support::paths::RustockerPaths::image_store_dir();
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
        Commands::Layouts => {
            let store = rustocker::engine::support::paths::RustockerPaths::layout_store_dir();
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
        Commands::Ps => {
            // rustocker::engine::runtime::refresh::refresh_container_states().await.expect("[ERROR] Failed to refresh container states");
            let container_dir = RustockerPaths::runtime_dir();
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
        Commands::Stop { id } => {
            let runtime_dir = RustockerPaths::runtime_dir().join(&id);
            let target_pid = find_pid(&id, &runtime_dir);

            let config_str = std::fs::read_to_string(runtime_dir.join("config.json")).expect("[ERROR] Failed to read config");
            let mut config = serde_json::from_str::<RuntimeConfig>(&config_str).expect("[ERROR] Failed to parse config");

            if config.status == ContainerStatus::Active {
                let ipam: Ipam = Ipam::new("172.19.0.0/16", RustockerPaths::base_dir().join("ipam.json")).unwrap();

                stop_container(ipam, &id, target_pid, &runtime_dir, PathBuf::from("/sys/fs/cgroup"), &mut config).await.expect("[ERROR] Failed to stop container");
                config.status = ContainerStatus::Stopped;

                std::fs::write(
                    &runtime_dir.join("config.json"),
                    serde_json::to_string_pretty(&config)
                        .unwrap()
                        .as_bytes(),
                ).expect("[ERROR] Failed to write config");
            }
        }
        Commands::Rm { id } => {
            if id != "." {
                handle_deletion_of_container(id).await;
            } else {
                print!("Are you sure to delete all stopped containers? [y/n]");
                let g = getch_rs::Getch::new();

                loop {
                    match g.getch() {
                        Ok(Key::Char('n')) => { break; }
                        Ok(Key::Char('y')) => {
                            if let Ok(dirs) = std::fs::read_dir(RustockerPaths::runtime_dir()) {
                                for entry in dirs.flatten() {
                                    let path = entry.path();
                                    let name = path
                                        .file_name()
                                        .unwrap()
                                        .to_str()
                                        .unwrap()
                                        .to_string();

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
            id,
            interactive,
            tty,
            cmd,
            args
        } => {
            let runtime_dir = RustockerPaths::runtime_dir().join(&id);
            let target_pid = find_pid(&id, runtime_dir);
  
            let opts = ExecOptions {
                interactive,
                tty,
                cmd,
                args
            };

            handle_exec(target_pid, id, opts)
                .await
                .expect("[ERROR] Failed to execute handle_exec");
        }
        Commands::Refresh => {
            rustocker::engine::runtime::refresh::refresh_container_states().await.expect("[ERROR] Failed to refresh container states");
        }
    }
}

async fn handle_deletion_of_container(id: String) {
    let runtime_dir = RustockerPaths::runtime_dir().join(&id);

    if let Ok(content) = std::fs::read_to_string(runtime_dir.join("config.json")) {
        if let Ok(config) = serde_json::from_str::<RuntimeConfig>(&content) {
            if config.status == ContainerStatus::Active {
                eprintln!("[ERROR] Active container cannot be deleted. Stop it first");
                std::process::exit(1);
            }

            if let Err(e) = std::fs::remove_dir_all(&runtime_dir) {
                eprintln!("[WARN] Failed to remove container dir: {}", e);
            } else {
                println!("{}", id);
            }
        }
    }
}

fn find_pid<'a, P>(id: &String, container_dir: P) -> i32
where P: Into<Cow<'a, Path>>
{
    let mut target_pid: i32 = 0;

    if let Ok(content) = std::fs::read_to_string(container_dir.into().join("config.json")) {
        match serde_json::from_str::<RuntimeConfig>(content.as_str()) {
            Ok(config) => {
                target_pid = config.pid;
            }
            Err(_) => eprintln!("[WARN] Failed to retrieve data for {}", &id),
        }
    }

    target_pid
}

pub async fn handle_exec(container_pid: i32, container_id: String, opts: ExecOptions) -> Result<(), Box<dyn std::error::Error>> {
    tokio::task::spawn_blocking(move || {
        rustocker::engine::runtime::exec::exec_in_container(
            container_pid,
            container_id,
            opts
        ).expect("[ERROR] Failed to exec in container");
    }).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_run(args: &[&str]) -> Commands {
        let mut full = vec!["rustocker", "run"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).unwrap().command
    }

    #[test]
    fn run_parses_all_parameters() {
        match parse_run(&[
            "my-layout",
            "-n",
            "MyContainer",
            "--rm",
            "-d",
            "-C",
            "1.5",
            "-M",
            "2048",
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
            } => {
                assert_eq!(layout, "my-layout");
                assert_eq!(cpu_limit, Some(1.5));
                assert_eq!(memory_limit, Some(2048.0));
                assert_eq!(command, Some("/bin/sh".to_string()));
                assert_eq!(args, None);
                assert_eq!(detach, true);
                assert_eq!(name, Some("MyContainer".to_string()));
                assert_eq!(rm, true);
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
            } => {
                assert_eq!(layout, "my-layout");
                assert_eq!(cpu_limit, None);
                assert_eq!(memory_limit, None);
                assert_eq!(command, None);
                assert_eq!(args, None);
                assert_eq!(detach, false);
                assert_eq!(name, None);
                assert_eq!(rm, false);
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
        let result = Cli::try_parse_from(["rustocker", "run", "my-layout", "some-arg"]);
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
            "rustocker",
            "build",
            "-f",
            "Rustockerfile.dev",
            "-t",
            "my-image",
        ])
        .unwrap();
        match cli.command {
            Commands::Build { file, tag } => {
                assert_eq!(file, "Rustockerfile.dev");
                assert_eq!(tag, "my-image");
            }
            _ => panic!("expected Build command"),
        }
    }

    #[test]
    fn build_uses_default_file() {
        let cli = Cli::try_parse_from(["rustocker", "build", "-t", "my-image"]).unwrap();
        match cli.command {
            Commands::Build { file, .. } => assert_eq!(file, "Rustockerfile"),
            _ => panic!("expected Build command"),
        }
    }
}
