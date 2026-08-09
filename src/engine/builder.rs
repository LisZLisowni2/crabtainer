use crate::engine::rustockerfile::{Instruction, Rustockerfile};
use crate::engine::instructions::download::download_image_if_missing;
use crate::engine::instructions::from::from_image;
use std::path::{Path, PathBuf};
use crate::engine::instructions::copy::copy;
use crate::engine::instructions::run::run_in_container;
use crate::engine::paths::RustockerPaths;

struct LayoutOpts {
    rootfs: PathBuf,
    cmd: Vec<String>,
}

pub async fn build_layout(rustocker_file: String, output_layout_name: String) -> Result<(), String> {
    let rustocker_path = Path::new(rustocker_file.as_str());

    let rustocker = Rustockerfile::parse_from_file(rustocker_path)
        .expect("Error parsing rustocker");

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
        rootfs: output_path.join("rootfs"),
        cmd: vec![]
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
                copy(src, dst, &output_layout_name).await?;
            }
            Instruction::Run(command) => {
                println!(" => [{}/{}] RUN {}", count, steps, command);
                run_in_container(&output_layout_name, command).await?;
            },
            Instruction::Cmd(cmd) => {
                println!(" => [{}/{}] CMD {:?}", count, steps, cmd);
                opts.cmd = cmd;
            }
        }
    }

    println!("[BUILDER] Instruction done");
    
    Ok(())
}