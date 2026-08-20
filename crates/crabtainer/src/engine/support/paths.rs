use std::fs;
use std::path::PathBuf;

pub struct CrabtainerPaths;

impl CrabtainerPaths {
    pub fn base_dir() -> PathBuf {
        std::env::var("CRABTAINER_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/var/lib/crabtainer"))
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
        let dirs = [
            Self::image_store_dir(),
            Self::runtime_dir(),
            Self::layout_store_dir(),
        ];

        for dir in &dirs {
            fs::create_dir_all(dir)
                .map_err(|e| format!("Error creating directory {:?}: {}", dir, e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::support::test_utils::{with_home, without_home};

    #[test]
    fn base_dir_defaults_to_var_crabtainer() {
        without_home(|| {
            assert_eq!(
                CrabtainerPaths::base_dir(),
                PathBuf::from("/var/lib/crabtainer")
            );
        });
    }

    #[test]
    fn base_dir_honors_crabtainer_home() {
        let home = std::env::temp_dir().join("crabtainer-test-home");
        with_home(home.to_str().unwrap(), || {
            assert_eq!(CrabtainerPaths::base_dir(), home);
        });
    }

    #[test]
    fn subdirs_are_relative_to_base() {
        let home = std::env::temp_dir().join("crabtainer-test-layout");
        with_home(home.to_str().unwrap(), || {
            assert_eq!(CrabtainerPaths::image_store_dir(), home.join("images"));
            assert_eq!(CrabtainerPaths::layout_store_dir(), home.join("layouts"));
            assert_eq!(CrabtainerPaths::runtime_dir(), home.join("containers"));
        });
    }

    #[test]
    fn init_system_dirs_creates_all_dirs() {
        let home = std::env::temp_dir().join("crabtainer-test-crabtainer_init");
        with_home(home.to_str().unwrap(), || {
            CrabtainerPaths::init_system_dirs().unwrap();
            assert!(CrabtainerPaths::image_store_dir().is_dir());
            assert!(CrabtainerPaths::layout_store_dir().is_dir());
            assert!(CrabtainerPaths::runtime_dir().is_dir());
        });
        let _ = std::fs::remove_dir_all(&home);
    }
}
