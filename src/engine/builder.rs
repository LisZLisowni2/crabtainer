use crate::engine::rustockerfile::{parse_memory_limit, Instruction, Rustockerfile};
use crate::engine::instructions::download::download_image_if_missing;
use crate::engine::instructions::from::from_image;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::BufWriter;
use crate::engine::instructions::copy::{copy_to_layout};
use crate::engine::instructions::run::run_in_container;
use crate::engine::paths::RustockerPaths;

#[derive(Serialize, Deserialize, Debug)]
pub struct LayoutOpts {
    pub memory_limit: Option<f64>,
    pub cpu_limit: Option<f64>,
    pub cmd: Option<String>,
    pub args: Vec<String>,
}

pub async fn build_layout(rustocker_file: String, output_layout_name: String) -> Result<(), String> {
    let rustocker_path = Path::new(rustocker_file.as_str());

    let rustocker = Rustockerfile::parse_from_file(rustocker_path)?;

    println!("[BUILDER] Building an layout: {}", output_layout_name);
    println!("[BUILDER] Create a dir for layout: {}", output_layout_name);
    
    let output_path = RustockerPaths::layout_store_dir().join(&output_layout_name);

    if std::fs::metadata(&output_path).is_ok() {
        println!("[WARN] Layout {} already exists!", output_layout_name);
    }
    let _ = std::fs::create_dir_all(&output_path);

    let mut count = 0;
    let steps = rustocker.instructions.len();
    let mut opts = LayoutOpts {
        memory_limit: None,
        cpu_limit: None,
        cmd: None,
        args: vec![],
    };
    
    for instruction in rustocker.instructions {
        count += 1;
        match instruction {
            Instruction::Download {url, alias} => {
                println!(" => [{}/{}] DOWNLOAD {} AS {}", count, steps, url, alias);
                download_image_if_missing(&url, &alias).await?;
            },
            Instruction::From(base_image) => {
                println!(" => [{}/{}] FROM {}", count, steps, base_image);
                from_image(&base_image, &output_layout_name).await?;
            },
            Instruction::Copy {src, dst} => {
                println!(" => [{}/{}] COPY {} to {}", count, steps, src, dst);
                copy_to_layout(src.as_str(), dst.as_str(), &output_layout_name).await?;
            }
            Instruction::Run(command) => {
                println!(" => [{}/{}] RUN {}", count, steps, command);
                run_in_container(&output_layout_name, command).await?;
            },
            Instruction::Cmd  { cmd, args } => {
                println!(" => [{}/{}] CMD {:?} {:?}", count, steps, cmd, args);
                opts.cmd = Some(cmd);
                opts.args = args.clone();
            },
            Instruction::CpuLimit(cores) => {
                println!(" => [{}/{}] CPU LIMIT {}", count, steps, cores);
                opts.cpu_limit = Some(cores);
            },
            Instruction::MemoryLimit(limit) => {
                println!(" => [{}/{}] MEMORY LIMIT {}", count, steps, limit);
                let bytes = parse_memory_limit(limit.as_str())?;
                opts.memory_limit = Some(bytes);
            }
        }
    }

    println!("[BUILDER] Instruction done. Injecting config.");

    let json_string = serde_json::to_string_pretty(&opts).unwrap();
    std::fs::write(output_path.join("config.json"), json_string).unwrap();

    Ok(())
}