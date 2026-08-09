use std::fs;
use std::fs::{DirEntry, ReadDir};
use std::path::{Path, PathBuf};
use crate::engine::paths::RustockerPaths;
use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

pub struct RustockerIgnore {
    rules: Vec<(GlobSet, bool)>,
    root: PathBuf,
}

impl RustockerIgnore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let ignore_file = root.join(".rustockerignore");
        let mut rules = Vec::new();

        if let Ok(content) = fs::read_to_string(&ignore_file) {
            for line in content.lines() {
                let line = line.trim();

                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let (raw_pattern, is_negated) = if let Some(stripped) = line.strip_prefix('!') {
                    (stripped, true)
                } else {
                    (line, false)
                };

                let pattern_str = if !raw_pattern.contains('*') && !raw_pattern.contains('?') {
                    format!("{0}{{{0},/**}}", raw_pattern.trim_start_matches('/'))
                } else {
                    raw_pattern.trim_start_matches('/').to_string()
                };

                let mut builder = GlobSetBuilder::new();

                if let Ok(glob) = Glob::new(&pattern_str) {
                    builder.add(glob);
                    if let Ok(globset) = builder.build() {
                        rules.push((globset, is_negated));
                    }
                }
            }
        }

        Self { rules, root }
    }

    pub fn is_ignored(&self, relative_path: &Path) -> bool {
        if relative_path == Path::new(".rustockerignore") {
            return true;
        }

        let mut ignored = false;
        for (globset, is_negated) in &self.rules {
            if globset.is_match(relative_path) {
                ignored = !is_negated;
            }
        }

        ignored
    }

    pub fn collect_files(&self, source_dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();

        let walker = WalkDir::new(source_dir).into_iter();

        for entry in walker.filter_entry(|e| {
            if e.path() == source_dir {
                return true;
            }
            if let Ok(rel) = e.path().strip_prefix(&self.root) {
                !self.is_ignored(rel)
            } else {
                true
            }
        }) {
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Ok(rel) = entry.path().strip_prefix(&self.root) {
                        files.push(entry.into_path());
                    }
                }
            }
        }

        files
    }
}

pub async fn copy_to_layout(src: &str, dst: &str, output_layout_name: &str) -> Result<(), String> {
    let dst_relative = Path::new(&dst)
        .strip_prefix("/")
        .unwrap_or(Path::new(&dst));

    let destination = RustockerPaths::layout_store_dir()
        .join(output_layout_name)
        .join("rootfs")
        .join(&dst_relative);

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| format!(" => [COPY] Failed to create directories: {}", err.to_string()))?;
    }

    let mut ignore: Vec<String> = Vec::new();
    if fs::metadata(".rustockerignore").is_ok() {
        let splited: Vec<String> = fs::read_to_string(".rustockerignore")
            .expect(" => [COPY] Failed to open .rustockerignore")
            .split("\n")
            .map(|s| s.to_string())
            .collect();

        ignore.extend(splited);
    }

    println!("[COPY] {} -> {}", &src, &destination.display());

    let ignore_engine = RustockerIgnore::new(".");

    // Case 1: Universal wildcard (Copy entire workspace respecting ignores)
    if src == "*" {
        let files_to_copy = ignore_engine.collect_files(Path::new("."));

        for rel_path in files_to_copy {
            let target_path = destination.join(&rel_path);

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!(" => [COPY] Failed to create dir {}: {}", parent.display(), e))?;
            }

            fs::copy(&rel_path, &target_path)
                .map_err(|e| format!(" => [COPY] Failed to copy {}: {}", rel_path.display(), e))?;
        }
        return Ok(());
    }

    // Case 2 & 3: Glob expansion or direct paths
    let entries = if src.contains('*') || src.contains('?') || src.contains('[') {
        // Expand glob pattern
        glob::glob(src)
            .map_err(|e| format!(" => [COPY] Invalid glob pattern '{}': {}", src, e))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>()
    } else {
        // Single static path
        vec![Path::new(&src).to_path_buf()]
    };

    if entries.is_empty() {
        println!(" => [COPY] Warning: No files matched pattern '{}'", src);
        return Ok(());
    }

    for src_path in entries {
        // Normalize path relative to workspace root for ignore checking
        let rel_path = src_path
            .strip_prefix("./")
            .unwrap_or(&src_path);

        // Skip ignored paths
        if ignore_engine.is_ignored(rel_path) {
            println!(" => [COPY] Skipping ignored path: {}", rel_path.display());
            continue;
        }

        if src_path.is_dir() {
            // Recursively collect and copy directory contents
            let files = ignore_engine.collect_files(&src_path);
            for rel_file in files {
                let target_path = destination.join(&rel_file);
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!(" => [COPY] Failed to create dir {}: {}", parent.display(), e))?;
                }
                fs::copy(&rel_file, &target_path)
                    .map_err(|e| format!(" => [COPY] Failed to copy {}: {}", rel_file.display(), e))?;
            }
        } else if src_path.is_file() {
            // Replicate relative structure under destination
            let target_path = destination.join(rel_path);

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!(" => [COPY] Failed to create dir {}: {}", parent.display(), e))?;
            }

            fs::copy(&src_path, &target_path)
                .map_err(|e| format!(" => [COPY] Failed to copy {}: {}", src_path.display(), e))?;
        }
    }

    Ok(())
}