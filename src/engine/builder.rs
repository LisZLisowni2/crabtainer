use crate::engine::rockerfile::{Instruction, Rockerfile};
use crate::engine::instructions::download::download_image_if_missing;
use crate::engine::instructions::from::from_image;
use std::path::{Path, PathBuf};
use crate::engine::paths::RustockerPaths;

struct LayoutOpts {
    rootfs: PathBuf,
    cmd: Vec<String>,
}

pub async fn build_image(rockerfile_str: String, output_layout_name: String) -> Result<(), String> {
    let rockerfile_path = Path::new(rockerfile_str.as_str());

    let rockerfile = Rockerfile::parse_from_file(rockerfile_path)
        .expect("Error parsing rockerfile");

    println!("[BUILDER] Building an layout: {}", output_layout_name);
    println!("[BUILDER] Create a dir for layout: {}", output_layout_name);

    let _ = std::fs::create_dir_all(RustockerPaths::layout_store_dir().join(&output_layout_name));

    let mut count = 0;
    let steps = rockerfile.instructions.len();
    let mut opts = LayoutOpts {
        rootfs: RustockerPaths::layout_store_dir().join(&output_layout_name).join("rootfs"),
        cmd: vec![]
    };
    
    for instruction in rockerfile.instructions {
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
                // TODO: Copy file from host to rootfs in /tmp
            }
            Instruction::Run(command) => {
                println!(" => [{}/{}] RUN {}", count, steps, command);
                // TODO: Run temporary container chroot, execute command and save to layer
            },
            Instruction::Cmd(cmd) => {
                println!(" => [{}/{}] CMD {:?}", count, steps, cmd);
                opts.cmd = cmd;
            }
        }
    }
    
    
    
    Ok(())
}