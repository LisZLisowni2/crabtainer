use std::os::fd::{AsFd, AsRawFd};
use clap::{Parser, Subcommand};
use nix::sched::CloneFlags;
use rustocker::engine::build::builder::build_layout;
use rustocker::engine::runtime::container::{run_container, spawn_detach_container};
use rustocker::engine::runtime::options::{ContainerOptions, RuntimeConfig};
use rustocker::engine::support::paths::RustockerPaths;

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
        name: String,
    },
    Rm {
        name: String,
    },
    Exec {
        name: String,

        #[arg(short, long, default_value_t = false)]
        interactive: bool,

        #[arg(short, long, default_value_t = false)]
        tty: bool,
    },
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
                rm
            };

            let container_id = rustocker::engine::runtime::container::generate_container_id();
            
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
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
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
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    println!("{:<20} {:<15}", name, size / 1024 / 1024);
                }
            }
        }
        Commands::Ps => {
            let container_dir = RustockerPaths::runtime_dir();
            println!("{:<15} {:<20} {:<20} {:<15}", "ID", "NAME", "LAYOUT", "STATUS");
            println!("{}", "-".repeat(80));
            if let Ok(entries) = std::fs::read_dir(container_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let id = path
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap();

                    let runtime_config_path = path.join("config.json");
                    if let Ok(file) = std::fs::read_to_string(runtime_config_path) {
                        match serde_json::from_str::<RuntimeConfig>(file.as_str()) {
                            Err(_) => eprintln!("[WARN] Failed to retrieve data for {}", id),
                            Ok(data) => println!("{:<15} {:<20} {:<20} {:<15}", id, data.container_name, data.layout_name, data.status),
                        }
                    } else {
                        eprintln!("[WARN] Failed to retrieve data for {}", id);
                    }
                }
            }
        }
        Commands::Stop { name } => {

        }
        Commands::Rm { name } => {

        }
        Commands::Exec { name, interactive, tty } => {
            let runtime_dir = RustockerPaths::runtime_dir();

            let namespaces = [
                ("ipc", CloneFlags::CLONE_NEWIPC),
                ("uts", CloneFlags::CLONE_NEWUTS),
                ("net", CloneFlags::CLONE_NEWNET),
                ("pid", CloneFlags::CLONE_NEWPID),
                ("mnt", CloneFlags::CLONE_NEWNS),
            ];

            for (ns_name, flag) in namespaces {
                let target_pid = 122842; // temporary
                let ns_path = format!("/proc/{}/ns/{}", target_pid, ns_name);
                if let Ok(file) = std::fs::File::open(&ns_path) {
                    nix::sched::setns(file.as_fd(), flag).unwrap();
                }
            }
        }
    }
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
        match parse_run(&["my-layout", "-n", "MyContainer", "--rm", "-d", "-C", "1.5", "-M", "2048", "-c", "/bin/sh"]) {
            Commands::Run {
                layout,
                cpu_limit,
                memory_limit,
                command,
                args,
                name,
                detach,
                rm
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
                rm
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
