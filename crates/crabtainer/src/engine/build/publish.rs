//! # Publish handler
//!
//! This module converts layout to image to use it as reusable component.

use crate::engine::support::paths::CrabtainerPaths;

pub async fn publish_layout(tag: String) {
    let store = CrabtainerPaths::layout_store_dir();
    let path = store.join(tag);
    let rootfs = path.join("rootfs");
    if let Some(root) = rootfs.to_str() {
        match tokio::process::Command::new("tar")
            .args(["-czvf", "layer_0.tar.gz", root])
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
    } else {
        eprintln!("[CRITICAL] Failed to convert rootfs to &str");
    }
}
