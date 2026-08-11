use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct EnvScope {
    _guard: MutexGuard<'static, ()>,
    home: tempfile::TempDir,
}

pub fn isolated_home() -> EnvScope {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().unwrap();
    // SAFETY: serialized behind ENV_LOCK so no other thread touches this env var.
    unsafe { std::env::set_var("RUSTOCKER_HOME", home.path()) };
    EnvScope {
        _guard: guard,
        home,
    }
}

impl EnvScope {
    pub fn home(&self) -> &Path {
        self.home.path()
    }
}

impl Drop for EnvScope {
    fn drop(&mut self) {
        // SAFETY: the ENV_LOCK guard is still held.
        unsafe { std::env::remove_var("RUSTOCKER_HOME") };
    }
}

#[allow(dead_code)]
pub fn create_tarball(home: &Path, base_image_alias: &str, entries: &[(&str, &str)]) {
    let img_dir = home.join("images");
    std::fs::create_dir_all(&img_dir).unwrap();

    let src = home.join("tarball-src");
    let _ = std::fs::remove_dir_all(&src);
    for (path, content) in entries {
        let full = src.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    let tar_path = img_dir.join(format!("{}.tar.gz", base_image_alias));
    let status = std::process::Command::new("tar")
        .args([
            "-czf",
            tar_path.to_str().unwrap(),
            "-C",
            src.to_str().unwrap(),
            ".",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "failed to create test tarball");
    let _ = std::fs::remove_dir_all(&src);
}
