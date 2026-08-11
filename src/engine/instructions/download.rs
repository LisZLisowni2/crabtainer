use crate::engine::paths::RustockerPaths;
use futures_util::StreamExt;
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub async fn download_image_if_missing(
    url: &str,
    alias: &str,
) -> Result<std::path::PathBuf, String> {
    RustockerPaths::init_system_dirs()?;

    let storage_dir = RustockerPaths::image_store_dir();

    let target_path = storage_dir.join(format!("{}.tar.gz", alias));
    let temp_path = storage_dir.join(format!("{}.tar.gz.temp", alias));

    if target_path.exists() {
        println!(" => [DOWNLOAD] Image '{}' already exists", alias);
        return Ok(target_path);
    }

    println!(" => [DOWNLOAD] Downloading image from url {}...", url);

    let client = Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Error downloading image: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Error downloading image: {}",
            response.text().await.unwrap()
        ));
    }

    let total_size = response.content_length().unwrap_or(0);

    let pb = indicatif::ProgressBar::new(total_size);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );

    let mut file = File::create(&temp_path)
        .await
        .map_err(|e| format!("Error creating file: {}", e))?;

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(e) => {
                let _ = std::fs::remove_file(&temp_path).unwrap();
                return Err(format!("Error downloading image: {}", e));
            }
        };

        if let Err(e) = file.write_all(&chunk).await {
            let _ = std::fs::remove_file(&temp_path);
            return Err(format!("Error writing to file: {}", e));
        }

        pb.inc(chunk.len() as u64);
    }

    if let Err(e) = file.flush().await {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("Error flushing file: {}", e));
    }

    pb.finish_with_message("Download complete");

    std::fs::rename(&temp_path, &target_path)
        .map_err(|e| format!("Error renaming file: {}", e))?;

    println!(" => [DOWNLOAD] Image '{}' successfully downloaded", alias);

    Ok(target_path)
}
