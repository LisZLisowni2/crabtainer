use std::path::PathBuf;
use tokio::process::Command;
use crate::engine::paths::RustockerPaths;

pub async fn from_image(base_image: &String, output_layout_name: &String) -> Result<(), String> {
    let files = std::fs::read_dir(RustockerPaths::image_store_dir()).map_err(|e| format!("Error reading directory: {}", e))?;

    let mut found = false;
    let mut found_file: PathBuf = PathBuf::new();
    for file in files.flatten() {
        let name = file.file_name().to_string_lossy().replace(".tar.gz", "");

        if name == base_image.as_str() {
            found = true;
            found_file = file.path();
            break;
        }
    }

    if !found {
        return Err(format!(" => [FROM] Image {} can not be found", base_image));
    }

    println!(" => [FROM] Extract rootfs from {} to {}'s rootfs", base_image, output_layout_name);
    let path = RustockerPaths::layout_store_dir().join(output_layout_name).join("rootfs");

    std::fs::create_dir_all(&path).map_err(|e| format!("Error creating output directory: {}", e))?;

    // TODO: Handle other types of zip (like .zip)
    let status = Command::new("tar")
        .args(["-xzf", &found_file.to_string_lossy(), "-C", &path.to_str().unwrap()])
        .status()
        .await
        .map_err(|e| format!(" => [FROM] Error spwaning tar: {}", e))?;

    if !status.success() {
        return Err(String::from(" => [FROM] Error extracting rootfs"));
    }

    Ok(())
}