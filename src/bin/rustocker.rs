use clap::{Parser, Subcommand};
use nix::libc::exit;
use rustocker::engine::container::{run_container, ContainerOptions};
use rustocker::engine::builder::{build_layout};

#[derive(Parser)]
#[command(name = "rustocker")]
#[command(about = "Rustocker - A lightweight container runtime built from scratch in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands
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

        #[arg(trailing_var_arg = true, allow_hyphen_values = true, requires = "command")]
        args: Vec<String>,
    },
    Build {
        #[arg(short, long, default_value = "Rustockerfile")]
        file: String,

        #[arg(short, long)]
        tag: String,
    },
    Images,
    Layouts
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if !is_root::is_root() {
        eprintln!("You have to run this command with sudo privileges");
        return;
    }

    match cli.command {
        Commands::Run { layout, command, args, cpu_limit, memory_limit } => {
            let options = ContainerOptions { layout_name: layout, command, args, cpu_limit, memory_limit };
            let _ = run_container(options).await.unwrap();
        },
        Commands::Build { file, tag } => {
            build_layout(file, tag).await.unwrap();
        },
        Commands::Images => {
            let store = rustocker::engine::paths::RustockerPaths::image_store_dir();
            println!("{:<20} {:<15}", "ALIAS", "SIZE");
            println!("{}", "-".repeat(38));
            let status = std::fs::read_dir(&store);

            if let Ok(entries) = std::fs::read_dir(store) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_stem().unwrap().to_string_lossy().replace(".tar.gz", "");
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    println!("{:<20} {:<15} MB", name, size / 1024 / 1024);
                }
            }
        },
        Commands::Layouts => {
            let store = rustocker::engine::paths::RustockerPaths::layout_store_dir();
            println!("{:<20} {:<15}", "LAYOUT TAG", "SIZE");
            println!("{}", "-".repeat(38));

            if let Ok(entries) = std::fs::read_dir(store) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_stem().unwrap().to_string_lossy().replace(".tar.gz", "");
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    println!("{:<20} {:<15}", name, size / 1024 / 1024);
                }
            }
        }
    }
}