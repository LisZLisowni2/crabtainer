use std::fs;
use std::path::PathBuf;

pub struct RockerPaths;

impl RockerPaths {
    pub fn base_dir() -> PathBuf {
        PathBuf::from("/var/rocker")
    }

    pub fn image_store_dir() -> PathBuf {
        Self::base_dir().join("images")
    }

    pub fn layout_store_dir() -> PathBuf {
        Self::base_dir().join("layouts")
    }

    pub fn runtime_dir() -> PathBuf {
        Self::base_dir().join("containers")
    }

    pub fn init_system_dirs() -> Result<(), String> {
        let dirs = [Self::image_store_dir(), Self::runtime_dir(), Self::layout_store_dir()];

        for dir in &dirs {
            fs::create_dir_all(dir).map_err(|e| format!("Error creating directory {:?}: {}", dir, e))?;
        }

        Ok(())
    }
}