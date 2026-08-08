use crate::engine::rockerfile::{Instruction, Rockerfile};
use crate::engine::download::download_image_if_missing;
use std::path::Path;

pub async fn build_image(rockerfile_str: String, output_image_name: String) -> Result<(), String> {
    let rockerfile_path = Path::new(rockerfile_str.as_str());

    let rockerfile = Rockerfile::parse_from_file(rockerfile_path)
        .expect("Error parsing rockerfile");

    println!("[BUILDER] Building an image: {}", output_image_name);

    let mut count = 0;
    let steps = rockerfile.instructions.len();

    for instruction in rockerfile.instructions {
        count += 1;
        match instruction {
            Instruction::Download {url, alias} => {
                println!(" => [{}/{}] DOWNLOAD {} AS {}", count, steps, url, alias);
                download_image_if_missing(&url, &alias).await?;
            },
            Instruction::From(base_image) => {
                println!(" => [{}/{}] FROM {}", count, steps, base_image);
                // TODO: Read from local cache
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
                // TODO: Save metadata (how to run an image in 'rocker run')
            }
        }
    }
    
    Ok(())
}