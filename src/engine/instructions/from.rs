use crate::engine::paths::RustockerPaths;
use std::path::PathBuf;
use tokio::process::Command;

pub async fn from_image(base_image: &String, output_layout_name: &String) -> Result<(), String> {
    let image_dir_name = base_image.replace("/", "_").replace(".", "_");
    let image_path = RustockerPaths::image_store_dir().join(&image_dir_name);

    if !image_path.exists() || !image_path.is_dir() {
        return Err(format!(" => [FROM] Image '{}' not found in local store. Did you run DOWNLOAD?", base_image));
    }

    println!(
        " => [FROM] Extracting rootfs from image directory '{}' to layout '{}'",
        image_dir_name, output_layout_name
    );

    let target_rootfs = RustockerPaths::layout_store_dir()
        .join(&output_layout_name)
        .join("rootfs");

    std::fs::create_dir_all(&target_rootfs)
        .map_err(|e| format!("=> [FROM] Failed to create target rootfs directory: {}", e))?;

    let mut layer_files: Vec<PathBuf> = std::fs::read_dir(&image_path)
        .map_err(|e| format!("=> [FROM] Error reading image dir: {}", e))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with("layer_"))
                .unwrap_or(false)
        })
        .collect();

    layer_files.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| {
                name.to_string_lossy()
                    .strip_prefix("layer_")?
                    .strip_suffix(".tar.gz")?
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or(0)
    });

    if layer_files.is_empty() {
        return Err(format!(" => [FROM] No layers archive found in image store for {}", base_image));
    }

    for (idx, layer_path) in layer_files.iter().enumerate() {
        println!("    └─ Unpacking layer [{}/{}]: {:?}", idx + 1, layer_files.len(), layer_path.file_name().unwrap_or_default());

        let tar_gz = std::fs::File::open(&layer_path)
            .map_err(|e| format!("=> [FROM] Failed to open layer file '{:?}': {}", layer_path, e))?;

        let tar_decoder = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(tar_decoder);

        archive.set_preserve_permissions(true);
        archive.set_unpack_xattrs(true);

        for entry_result in archive.entries().map_err(|e| format!("=> [FROM] Failed to read layer: {}", e))? {
            let mut entry = entry_result.map_err(|e| format!("=> [FROM] Failed to read layer: {}", e))?;

            if let Err(e) = entry.unpack_in(&target_rootfs) {
                eprintln!("       [WARN] Non-fatal unpack issue on {:?}: {}", entry.path().unwrap_or_default(), e);
            }
        }

        let src_config = image_path.join("config.json");
        if src_config.exists() {
            let dest_config = RustockerPaths::layout_store_dir()
                .join(output_layout_name)
                .join("config.json");

            std::fs::copy(&src_config, &dest_config)
                .map_err(|e| format!("=> [FROM] Failed to copy layer config file: {}", e))?;
        }

        println!(" => [FROM] Successfully constructed rootfs for '{}'", output_layout_name);
    }

    Ok(())
}
