use crate::engine::paths::RustockerPaths;
use oci_client::client::{Client, ClientConfig};
use oci_client::secrets::RegistryAuth;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub async fn download_image_if_missing(
    image_ref: &str,
    alias: &str,
) -> Result<std::path::PathBuf, String> {
    RustockerPaths::init_system_dirs()?;

    let reference: oci_client::Reference = image_ref
        .parse()
        .map_err(|e| format!(" => [DOWNLOAD] Invalid image reference: {}", e))?;

    let image_dir = RustockerPaths::image_store_dir().join(&alias);

    std::fs::create_dir_all(&image_dir)
        .map_err(|e| format!("=> [DOWNLOAD] Failed to create image directory: {}", e))?;

    println!(" => [DOWNLOAD] Connecting to registry for {}...", image_ref);

    let client = Client::new(ClientConfig::default());
    let auth = RegistryAuth::Anonymous;

    println!(" => [DOWNLOAD] Fetching manifest...");
    let (manifest, _digest, config_json) = client
        .pull_manifest_and_config(&reference, &auth)
        .await
        .map_err(|e| format!("Failed to download manifest: {}", e))?;

    std::fs::write(image_dir.join("config.json"), config_json)
        .map_err(|e| format!("Failed to save config: {}", e))?;

    println!(" => [DOWNLOAD] Pulling {} layers...", manifest.layers.len());
    for (i, layer) in manifest.layers.iter().enumerate() {
        let layer_filename = format!("layer_{}.tar.gz", i);
        let layer_path = image_dir.join(&layer_filename);

        println!(
            "    └─ Layer [{}/{}]: {}",
            i + 1,
            manifest.layers.len(),
            &layer.digest[..12]
        );

        let mut layer_file = File::create(layer_path)
            .await
            .map_err(|e| format!("Failed to create layer file: {}", e))?;

        client
            .pull_blob(&reference, &*layer.digest, &mut layer_file)
            .await
            .map_err(|e| format!("Failed to pull layer blob: {}", e))?;

        layer_file
            .flush()
            .await
            .map_err(|e| format!("Failed to flush layer: {}", e))?;
    }

    println!(
        " => [DOWNLOAD] Image '{}' successfully fetched and stored!",
        alias
    );
    Ok(image_dir)
}
