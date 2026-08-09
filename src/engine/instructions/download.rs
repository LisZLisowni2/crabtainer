use futures_util::StreamExt;
use reqwest::Client;
use tokio::fs::{File};
use tokio::io::AsyncWriteExt;
use crate::engine::paths::RockerPaths;

pub async fn download_image_if_missing(url: &str, alias: &str) -> Result<std::path::PathBuf, String> {
    RockerPaths::init_system_dirs()?;

    let storage_dir = RockerPaths::image_store_dir();

    let target_path = storage_dir.join(format!("{}.tar.gz", alias));

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
        return Err(format!("Error downloading image: {}", response.text().await.unwrap()));
    }

    let total_size = response.content_length().unwrap_or(0);

    let pb = indicatif::ProgressBar::new(total_size);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precies}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );

    let mut file = File::create(&target_path)
        .await
        .map_err(|e| format!("Error creating file: {}", e))?;

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Error reading chunk: {}", e))?;

        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Error writing to file: {}", e))?;

        pb.inc(chunk.len() as u64);
    }

    file.flush().await.map_err(|e| format!("Error flushing file: {}", e))?;
    println!(" => [DOWNLOAD] Image '{}' successfully downloaded", alias);

    Ok(target_path)
}