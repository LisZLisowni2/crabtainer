use std::path::{Path, PathBuf};
use tokio::process::Command;
use crate::engine::paths::RustockerPaths;

pub async fn copy(src: String, dst: String, output_layout_name: &String) -> Result<(), String> {
    let dst_relative = Path::new(&dst)
        .strip_prefix("/")
        .unwrap_or(Path::new(&dst));

    let destination = RustockerPaths::layout_store_dir()
        .join(output_layout_name)
        .join("rootfs")
        .join(&dst_relative);
    std::fs::create_dir_all(&destination).map_err(|err| format!(" => [COPY] Failed to create directories: {}", err.to_string()))?;

    println!("[COPY] {} {}", src, &destination.display());
    // std::fs::copy(src, &destination).map_err(|err| format!(" => [COPY] Failed to copy file: {}", err.to_string()))?;

    Ok(())
}