use crate::engine::support::paths::CrabtainerPaths;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct CrabtainerIgnore {
    rules: Vec<(GlobSet, bool)>,
    root: PathBuf,
}

impl CrabtainerIgnore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let ignore_file = root.join("../../../../../../.crabtainerignore");
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

                let base = raw_pattern.trim_start_matches('/');

                let mut builder = GlobSetBuilder::new();

                if base.contains('*') || base.contains('?') {
                    if let Ok(glob) = Glob::new(base) {
                        builder.add(glob);
                    }
                } else {
                    let dir = base.trim_end_matches('/');
                    if let Ok(glob) = Glob::new(dir) {
                        builder.add(glob);
                    }
                    if let Ok(glob) = Glob::new(&format!("{}/**", dir)) {
                        builder.add(glob);
                    }
                }

                if let Ok(globset) = builder.build() {
                    rules.push((globset, is_negated));
                }
            }
        }

        Self { rules, root }
    }

    pub fn is_ignored(&self, relative_path: &Path) -> bool {
        if relative_path == Path::new("../../../../../../.crabtainerignore") {
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

        for entry in walker
            .filter_entry(|e| {
                if e.path() == source_dir {
                    return true;
                }
                if let Ok(rel) = e.path().strip_prefix(&self.root) {
                    !self.is_ignored(rel)
                } else {
                    true
                }
            })
            .flatten()
        {
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(&self.root)
                    .unwrap_or(entry.path());
                files.push(rel.to_path_buf());
            }
        }

        files
    }
}

pub async fn copy_to_layout(src: &str, dst: &str, output_layout_name: &str) -> Result<(), String> {
    let dst_relative = Path::new(&dst).strip_prefix("/").unwrap_or(Path::new(&dst));

    let destination = CrabtainerPaths::layout_store_dir()
        .join(output_layout_name)
        .join("rootfs")
        .join(dst_relative);

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!(" => [COPY] Failed to create directories: {}", err))?;
    }

    let mut ignore: Vec<String> = Vec::new();
    if fs::metadata("../../../../../../.crabtainerignore").is_ok() {
        let splited: Vec<String> = fs::read_to_string("../../../../../../.crabtainerignore")
            .expect(" => [COPY] Failed to open .crabtainerignore")
            .split("\n")
            .map(|s| s.to_string())
            .collect();

        ignore.extend(splited);
    }

    let ignore_engine = CrabtainerIgnore::new(".");

    // Case 1: Universal wildcard (Copy entire workspace respecting ignores)
    if src == "*" {
        let files_to_copy = ignore_engine.collect_files(Path::new("."));

        for rel_path in files_to_copy {
            let target_path = destination.join(&rel_path);

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        " => [COPY] Failed to create dir {}: {}",
                        parent.display(),
                        e
                    )
                })?;
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
        let rel_path = src_path.strip_prefix("./").unwrap_or(&src_path);

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
                    fs::create_dir_all(parent).map_err(|e| {
                        format!(
                            " => [COPY] Failed to create dir {}: {}",
                            parent.display(),
                            e
                        )
                    })?;
                }
                fs::copy(&rel_file, &target_path).map_err(|e| {
                    format!(" => [COPY] Failed to copy {}: {}", rel_file.display(), e)
                })?;
            }
        } else if src_path.is_file() {
            // Replicate relative structure under destination
            let target_path = destination.join(rel_path);

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        " => [COPY] Failed to create dir {}: {}",
                        parent.display(),
                        e
                    )
                })?;
            }

            fs::copy(&src_path, &target_path)
                .map_err(|e| format!(" => [COPY] Failed to copy {}: {}", src_path.display(), e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn write_file(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "content").unwrap();
    }

    fn ignore_with(root: &Path, rules: &str) -> CrabtainerIgnore {
        fs::write(root.join("../../../../../../.crabtainerignore"), rules).unwrap();
        CrabtainerIgnore::new(root)
    }

    fn collected(root: &Path, ig: &CrabtainerIgnore) -> Vec<PathBuf> {
        let mut files = ig.collect_files(root);
        files.sort();
        files
    }

    #[test]
    fn no_ignore_file_keeps_all_files() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "b.log");
        write_file(dir.path(), "sub/c.rs");

        let ig = CrabtainerIgnore::new(dir.path());
        assert_eq!(
            collected(dir.path(), &ig),
            vec![
                PathBuf::from("a.txt"),
                PathBuf::from("b.log"),
                PathBuf::from("sub/c.rs")
            ]
        );
    }

    #[test]
    fn crabtainerignore_itself_excluded_even_without_rules() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        fs::write(dir.path().join("../../../../../../.crabtainerignore"), "").unwrap();

        let ig = CrabtainerIgnore::new(dir.path());
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("a.txt")]);
    }

    #[test]
    fn file_pattern_ignores_at_any_depth() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "b.log");
        write_file(dir.path(), "sub/nested.txt");

        let ig = ignore_with(dir.path(), "*.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("b.log")]);
    }

    #[test]
    fn directory_rule_prunes_whole_subtree() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "target/debug/app");
        write_file(dir.path(), "src/main.rs");

        let ig = ignore_with(dir.path(), "target\n");
        assert_eq!(
            collected(dir.path(), &ig),
            vec![PathBuf::from("src/main.rs")]
        );
    }

    #[test]
    fn trailing_slash_directory_rule_ignores_dir() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "build/out.txt");
        write_file(dir.path(), "src/main.rs");

        let ig = ignore_with(dir.path(), "build/\n");
        assert_eq!(
            collected(dir.path(), &ig),
            vec![PathBuf::from("src/main.rs")]
        );
    }

    #[test]
    fn leading_slash_anchors_rule_to_root() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "node_modules/a.js");
        write_file(dir.path(), "sub/node_modules/b.js");

        let ig = ignore_with(dir.path(), "/node_modules\n");
        assert_eq!(
            collected(dir.path(), &ig),
            vec![PathBuf::from("sub/node_modules/b.js")]
        );
    }

    #[test]
    fn wildcard_pattern_ignores_matches() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "src/a.rs");
        write_file(dir.path(), "src/b.rs");
        write_file(dir.path(), "docs/readme.md");

        let ig = ignore_with(dir.path(), "src/*.rs\n");
        assert_eq!(
            collected(dir.path(), &ig),
            vec![PathBuf::from("docs/readme.md")]
        );
    }

    #[test]
    fn negation_reincludes_specific_file() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "keep.txt");
        write_file(dir.path(), "drop.txt");

        let ig = ignore_with(dir.path(), "*.txt\n!keep.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("keep.txt")]);
    }

    #[test]
    fn later_negation_overrides_previous_rule() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "keep.txt");

        let ig = ignore_with(dir.path(), "*\n!keep.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("keep.txt")]);
    }

    #[test]
    fn later_rule_overrides_previous_negation() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "keep.txt");

        let ig = ignore_with(dir.path(), "!keep.txt\n*\n");
        assert_eq!(collected(dir.path(), &ig), Vec::<PathBuf>::new());
    }

    #[test]
    fn negated_directory_reincludes_contents() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "build/main.rs");
        write_file(dir.path(), "keep/lib.rs");

        let ig = ignore_with(dir.path(), "*\n!keep/\n");
        assert_eq!(
            collected(dir.path(), &ig),
            vec![PathBuf::from("keep/lib.rs")]
        );
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "b.log");

        let ig = ignore_with(dir.path(), "# comment\n\n*.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("b.log")]);
    }
}
