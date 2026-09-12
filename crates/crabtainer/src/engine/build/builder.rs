use crate::engine::build::crabtainerfile::{Crabtainerfile, Instruction, parse_memory_limit};
use crate::engine::build::instructions::copy::copy_to_layout;
use crate::engine::build::instructions::download::download_image_if_missing;
use crate::engine::build::instructions::from::from_image;
use crate::engine::build::instructions::run::run_in_container;
use crate::engine::build::spec::{LayoutOpts, save_config};
use crate::engine::support::paths::CrabtainerPaths;
use std::path::Path;

pub async fn build_layout(
    crabtainer_file: String,
    output_layout_name: String,
) -> Result<(), String> {
    let crabtainer_path = Path::new(crabtainer_file.as_str());

    let crabtainer_parent_path = crabtainer_path
        .parent()
        .expect("Failed to retrieve parent directory");

    let mut crabtainer_parent_absolute_path;
    if !crabtainer_parent_path.is_empty() {
        crabtainer_parent_absolute_path = std::fs::canonicalize(crabtainer_parent_path)
            .expect("Failed to canonicalize crabtainer parent dir");
    } else {
        crabtainer_parent_absolute_path =
            std::fs::canonicalize(".").expect("Failed to canonicalize . dir");
    }

    let crabtainer = Crabtainerfile::parse_from_file(crabtainer_path)?;

    println!("[BUILDER] Building an layout: {}", output_layout_name);
    println!("[BUILDER] Create a dir for layout: {}", output_layout_name);

    let output_path = CrabtainerPaths::layout_store_dir().join(&output_layout_name);

    if std::fs::metadata(&output_path).is_ok() {
        println!("[WARN] Layout {} already exists!", output_layout_name);
    }

    tokio::fs::create_dir_all(&output_path)
        .await
        .map_err(|e| format!("Failed to create layout dir: {}", e))?;

    let mut count = 0;
    let steps = crabtainer.instructions.len();
    let mut opts = LayoutOpts {
        memory_limit: None,
        cpu_limit: None,
        args: vec![],
        workdir: None,
    };

    for instruction in crabtainer.instructions {
        count += 1;
        match instruction {
            Instruction::Download {
                image_ref,
                alias,
                is_override,
            } => {
                if !is_override {
                    println!(
                        " => [{}/{}] DOWNLOAD {} AS {}",
                        count, steps, image_ref, alias
                    );
                } else {
                    println!(
                        " => [{}/{}] DOWNLOAD {} AS {} OVERRIDE",
                        count, steps, image_ref, alias
                    );
                }
                download_image_if_missing(&image_ref, &alias).await?;
            }
            Instruction::From(base_image) => {
                println!(" => [{}/{}] FROM {}", count, steps, base_image);
                from_image(&base_image, &output_layout_name).await?;
            }
            Instruction::Copy { src, dst } => {
                println!(" => [{}/{}] COPY {} to {}", count, steps, src, dst);
                if dst == "~" {
                    if let Some(work_dst) = opts.workdir.clone() {
                        copy_to_layout(
                            src.as_str(),
                            work_dst.as_str(),
                            &output_layout_name,
                            &crabtainer_parent_absolute_path,
                        )
                        .await?;
                    } else {
                        copy_to_layout(
                            src.as_str(),
                            "/",
                            &output_layout_name,
                            &crabtainer_parent_absolute_path,
                        )
                        .await?;
                    }
                } else {
                    copy_to_layout(
                        src.as_str(),
                        dst.as_str(),
                        &output_layout_name,
                        &crabtainer_parent_absolute_path,
                    )
                    .await?;
                }
            }
            Instruction::Run(command) => {
                println!(" => [{}/{}] RUN {}", count, steps, command);
                run_in_container(&output_layout_name, opts.workdir.clone(), command).await?;
            }
            Instruction::Cmd { args } => {
                println!(" => [{}/{}] CMD {:?}", count, steps, args);
                opts.args = args.clone();
            }
            Instruction::CpuLimit(cores) => {
                println!(" => [{}/{}] CPU LIMIT {}", count, steps, cores);
                opts.cpu_limit = Some(cores);
            }
            Instruction::MemoryLimit(limit) => {
                println!(" => [{}/{}] MEMORY LIMIT {}", count, steps, limit);
                let bytes = parse_memory_limit(limit.as_str())?;
                opts.memory_limit = Some(bytes);
            }
            Instruction::Workdir(directory) => {
                println!(" => [{}/{}] WORKDIR {}", count, steps, directory);
                opts.workdir = Some(directory);
            }
        }
    }

    println!("[BUILDER] Instruction done. Saving config.");
    if let Err(e) = save_config(opts, output_path).await {
        eprintln!("[BUILDER] Failed to save config: {}", e);
    }

    Ok(())
}
