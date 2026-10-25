//! # Publish handler
//!
//! This module converts layout to image to use it as reusable component.

use crate::engine::support::paths::CrabtainerPaths;

pub async fn publish_layout(tag: String, name: String) {
    let store = CrabtainerPaths::layout_store_dir();
    let image_store = CrabtainerPaths::image_store_dir();
    let path = store.join(tag);
    let layer_save_path = path.join("layer_0.tar.gz");
    let layer_save_path_str = layer_save_path
        .to_str()
        .expect("[CRITICAL] Failed to convert PathBuf to &str");
    let image_path = image_store.join(name);
    let rootfs = path.join("rootfs");
    if let Some(root) = rootfs.to_str() {
        match tokio::process::Command::new("tar")
            .args(["-czvf", layer_save_path_str, root])
            .status()
            .await
        {
            Ok(code) => {
                if !code.success() {
                    eprintln!("[ERROR] Failed `tar` command: {}", code);
                }
            }
            Err(e) => {
                eprintln!("[ERROR] Failed to execute `tar` command: {}", e)
            }
        }

        if tokio::fs::metadata(&image_path).await.is_ok() {
            eprintln!("[ERROR] Image with that name already exists");
            return;
        }

        tokio::fs::create_dir_all(&image_path)
            .await
            .expect("[CRITICAL] Failed to create image store path");

        if tokio::fs::rename(layer_save_path, &image_path.join("layer_0.tar.gz"))
            .await
            .is_err()
        {
            tokio::fs::remove_dir_all(&image_path)
                .await
                .expect("[CRITICAL] Failed to delete broken image");
        }
    } else {
        eprintln!("[CRITICAL] Failed to convert rootfs to &str");
    }
}
