use crate::engine::instructions::copy::copy_to_layout;
use crate::engine::instructions::download::download_image_if_missing;
use crate::engine::instructions::from::from_image;
use crate::engine::instructions::run::run_in_container;
use crate::engine::paths::RustockerPaths;
use crate::engine::rustockerfile::{Instruction, Rustockerfile, parse_memory_limit};
use serde::{Deserialize, Serialize};
use std::path::{PathBuf, Path};
use oci_spec::runtime::{LinuxBuilder, LinuxCpuBuilder, LinuxMemoryBuilder, LinuxNamespaceBuilder, LinuxNamespaceType, LinuxResourcesBuilder, ProcessBuilder, RootBuilder, SpecBuilder};

#[derive(Serialize, Deserialize, Debug)]
pub struct LayoutOpts {
    pub memory_limit: Option<f64>,
    pub cpu_limit: Option<f64>,
    pub args: Vec<String>,
}

async fn save_config(opts: LayoutOpts, layout_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let period = 100000.0;
    let quota = if let Some(cpu) = opts.cpu_limit {
        cpu * period
    } else {
        period
    };

    let memory = if let Some(mem) = opts.memory_limit {
        mem
    } else {
        f64::MAX
    };

    let spec = SpecBuilder::default()
        .root(
            RootBuilder::default()
                .path("rootfs")
                .readonly(false)
                .build()?
        )
        .process(
            ProcessBuilder::default()
                .terminal(true)
                .args(opts.args)
                .build()?
        )
        .linux(
            LinuxBuilder::default()
                .namespaces(vec![
                    LinuxNamespaceBuilder::default().typ(LinuxNamespaceType::Pid).build()?,
                    LinuxNamespaceBuilder::default().typ(LinuxNamespaceType::Mount).build()?,
                    LinuxNamespaceBuilder::default().typ(LinuxNamespaceType::Uts).build()?,
                    LinuxNamespaceBuilder::default().typ(LinuxNamespaceType::Ipc).build()?,
                ])
                .resources(
                    LinuxResourcesBuilder::default()
                            .cpu(LinuxCpuBuilder::default()
                                .quota(quota as i64)
                                .period(period as u64)
                                .build()?
                            )
                            .memory(LinuxMemoryBuilder::default()
                                .limit(memory as i64)
                                .build()?
                            )
                        .build()?,
                )
                .build()?
        )
        .build()?;

    spec.save(layout_path.join("config.json"))?;

    Ok(())
}

pub async fn build_layout(
    rustocker_file: String,
    output_layout_name: String,
) -> Result<(), String> {
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
        args: vec![],
    };

    for instruction in rustocker.instructions {
        count += 1;
        match instruction {
            Instruction::Download { url, alias } => {
                println!(" => [{}/{}] DOWNLOAD {} AS {}", count, steps, url, alias);
                download_image_if_missing(&url, &alias).await?;
            }
            Instruction::From(base_image) => {
                println!(" => [{}/{}] FROM {}", count, steps, base_image);
                from_image(&base_image, &output_layout_name).await?;
            }
            Instruction::Copy { src, dst } => {
                println!(" => [{}/{}] COPY {} to {}", count, steps, src, dst);
                copy_to_layout(src.as_str(), dst.as_str(), &output_layout_name).await?;
            }
            Instruction::Run(command) => {
                println!(" => [{}/{}] RUN {}", count, steps, command);
                run_in_container(&output_layout_name, command).await?;
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
        }
    }

    println!("[BUILDER] Instruction done. Saving config.");
    if let Err(e) = save_config(opts, output_path).await {
        eprintln!("[BUILDER] Failed to save config: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_opts() -> LayoutOpts {
        LayoutOpts {
            memory_limit: Some(2048.0),
            cpu_limit: Some(1.5),
            args: vec!["/bin/sh".to_string(), "-c".to_string(), "echo hi".to_string()],
        }
    }

    #[test]
    fn layout_opts_serializes_round_trip() {
        let opts = sample_opts();
        let json = serde_json::to_string(&opts).unwrap();
        let decoded: LayoutOpts = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.memory_limit, Some(2048.0));
        assert_eq!(decoded.cpu_limit, Some(1.5));
        assert_eq!(decoded.args, vec!["/bin/sh".to_string(), "-c".to_string(), "echo hi".to_string()]);
    }
}
