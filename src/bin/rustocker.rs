use clap::{Parser, Subcommand};
use rustocker::engine::builder::build_layout;
use rustocker::engine::container::{ContainerOptions, run_container};

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

        #[arg(short = 'C', long)]
        cpu_limit: Option<f64>,

        #[arg(short = 'M', long)]
        memory_limit: Option<f64>,

        #[arg(short, long, default_value = "")]
        command: String,

        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            requires = "command"
        )]
        args: Vec<String>,
    },
    Build {
        #[arg(short, long, default_value = "Rustockerfile")]
        file: String,

        #[arg(short, long)]
        tag: String,
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
        } => {
            let options = ContainerOptions {
                layout_name: layout,
                command,
                args,
                cpu_limit,
                memory_limit,
            };
            run_container(options).await.unwrap();
        }
        Commands::Build { file, tag } => {
            build_layout(file, tag).await.unwrap();
        }
        Commands::Images => {
            let store = rustocker::engine::paths::RustockerPaths::image_store_dir();
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
            let store = rustocker::engine::paths::RustockerPaths::layout_store_dir();
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
    fn run_parses_cpu_and_memory_limits() {
        match parse_run(&["my-layout", "-C", "1.5", "-M", "2048", "-c", "/bin/sh"]) {
            Commands::Run {
                layout,
                cpu_limit,
                memory_limit,
                command,
                args,
            } => {
                assert_eq!(layout, "my-layout");
                assert_eq!(cpu_limit, Some(1.5));
                assert_eq!(memory_limit, Some(2048.0));
                assert_eq!(command, "/bin/sh");
                assert!(args.is_empty());
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
                ..
            } => {
                assert_eq!(layout, "my-layout");
                assert_eq!(cpu_limit, None);
                assert_eq!(memory_limit, None);
                assert_eq!(command, "");
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn run_collects_trailing_args() {
        match parse_run(&["my-layout", "-c", "/bin/sh", "echo", "hi"]) {
            Commands::Run { args, command, .. } => {
                assert_eq!(command, "/bin/sh");
                assert_eq!(args, vec!["echo", "hi"]);
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
                assert_eq!(command, "true");
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
