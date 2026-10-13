//! On-disk layout of the crabtainer stores.
//!
//! All paths are relative to a single base directory which defaults to
//! `/var/lib/crabtainer` and can be overridden with the `CRABTAINER_HOME`
//! environment variable (as used by the tests).

use std::fs;
use std::path::PathBuf;

/// Namespace for the root paths of every crabtainer store.
///
/// All methods are stateless and derive their result from the base directory.
pub struct CrabtainerPaths;

impl CrabtainerPaths {
    /// Returns the base directory for all crabtainer data.
    ///
    /// Honors the `CRABTAINER_HOME` environment variable and falls back to
    /// `/var/lib/crabtainer` when it is unset.
    pub fn base_dir() -> PathBuf {
        std::env::var("CRABTAINER_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/var/lib/crabtainer"))
    }

    /// Directory where downloaded images are stored (`<base>/images`).
    pub fn image_store_dir() -> PathBuf {
        Self::base_dir().join("images")
    }

    /// Directory where built layouts are stored (`<base>/layouts`).
    pub fn layout_store_dir() -> PathBuf {
        Self::base_dir().join("layouts")
    }

    /// Directory where container runtime state lives (`<base>/containers`),
    /// one subdirectory per container id.
    pub fn runtime_dir() -> PathBuf {
        Self::base_dir().join("containers")
    }

    /// Directory where named volumes are stored (`<base>/volumes`).
    pub fn volumes_dir() -> PathBuf {
        Self::base_dir().join("volumes")
    }

    /// Creates the `images`, `containers`, `layouts`, and `volumes`
    /// directories under the base dir, if they do not exist yet.
    ///
    /// # Errors
    ///
    /// Returns an error string naming the directory that could not be
    /// created.
    pub fn init_system_dirs() -> Result<(), String> {
        let dirs = [
            Self::image_store_dir(),
            Self::runtime_dir(),
            Self::layout_store_dir(),
            Self::volumes_dir(),
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
    /// `base_dir` falls back to `/var/lib/crabtainer` when the env var is
    /// unset.
    fn base_dir_defaults_to_var_crabtainer() {
        without_home(|| {
            assert_eq!(
                CrabtainerPaths::base_dir(),
                PathBuf::from("/var/lib/crabtainer")
            );
        });
    }

    #[test]
    /// `base_dir` honors the `CRABTAINER_HOME` env var.
    fn base_dir_honors_crabtainer_home() {
        let home = std::env::temp_dir().join("crabtainer-test-home");
        with_home(home.to_str().unwrap(), || {
            assert_eq!(CrabtainerPaths::base_dir(), home);
        });
    }

    #[test]
    /// Store subdirectories are resolved relative to the base dir.
    fn subdirs_are_relative_to_base() {
        let home = std::env::temp_dir().join("crabtainer-test-layout");
        with_home(home.to_str().unwrap(), || {
            assert_eq!(CrabtainerPaths::image_store_dir(), home.join("images"));
            assert_eq!(CrabtainerPaths::layout_store_dir(), home.join("layouts"));
            assert_eq!(CrabtainerPaths::runtime_dir(), home.join("containers"));
        });
    }

    #[test]
    /// `init_system_dirs` creates every store directory.
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
