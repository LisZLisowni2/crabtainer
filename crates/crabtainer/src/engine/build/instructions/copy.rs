//! The `COPY` build step: copying files into the layout's rootfs.
//!
//! Supports single paths, glob patterns, and the `*` wildcard for the whole
//! build directory. Copies honor `.crabtainerignore` rules (glob patterns,
//! negation with `!`, comments, and blank lines), implemented by
//! [`CrabtainerIgnore`].

use crate::engine::support::paths::CrabtainerPaths;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Evaluator for `.crabtainerignore` rules rooted at a build directory.
///
/// Rules are read from `<root>/.crabtainerignore`, one per line. A rule is a
/// glob pattern relative to `root`; a leading `!` negates it. Later rules
/// override earlier ones (including negations). The ignore file itself is
/// always ignored.
pub struct CrabtainerIgnore {
    /// Compiled rules as `(globset, is_negated)` pairs, in file order.
    rules: Vec<(GlobSet, bool)>,
    /// Build directory the rules are anchored to.
    root: PathBuf,
}

impl CrabtainerIgnore {
    /// Loads and compiles the `.crabtainerignore` file from `root`.
    ///
    /// Skips blank lines and `#` comments. Non-glob rules are expanded to
    /// match both the exact path and everything below it (like a directory
    /// rule). Returns an empty set of rules when no ignore file exists.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let ignore_file = root.join(".crabtainerignore");
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

                let base_trim = raw_pattern.trim_start_matches('/');
                let base_path = root.join(base_trim);
                let base = base_path
                    .to_str()
                    .expect("Failed to cast from PathBuf to &str");

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

        Self { root, rules }
    }

    /// Returns whether `absolute_path` is covered by the ignore rules.
    ///
    /// The `.crabtainerignore` file itself is always considered ignored. For
    /// every matching rule the verdict is updated: matched (non-negated)
    /// rules ignore, matched negated rules re-include, so the last matching
    /// rule wins.
    pub fn is_ignored(&self, absolute_path: &Path) -> bool {
        if absolute_path == self.root.join(".crabtainerignore") {
            return true;
        }

        let mut ignored = false;
        for (globset, is_negated) in &self.rules {
            if globset.is_match(absolute_path) {
                ignored = !is_negated;
            }
        }

        ignored
    }

    /// Walks `source_dir` recursively and returns the absolute paths of all
    /// regular files that are not ignored.
    ///
    /// Ignored directories are pruned entirely (never descended into).
    pub fn collect_files(&self, source_dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();

        let walker = WalkDir::new(source_dir).into_iter();

        for entry in walker
            .filter_entry(|e| {
                if e.path() == source_dir {
                    return true;
                }

                !self.is_ignored(e.path())
            })
            .flatten()
        {
            if entry.file_type().is_file() {
                let rel = entry.path();
                files.push(rel.to_path_buf());
            }
        }

        files
    }
}

/// Copies files from the build directory into the layout's rootfs.
///
/// Three source forms are supported:
///
/// 1. `*` — the whole build directory, respecting `.crabtainerignore`.
/// 2. A glob pattern (`*`, `?`, `[...]`), expanded via the [`glob`] crate.
/// 3. A single static path.
///
/// The relative path under the *build directory* is replicated under
/// `rootfs/<dst>/`. Directories are copied recursively; ignored paths are
/// skipped.
///
/// # Arguments
///
/// * `src` - Source path or pattern, relative to `build_dir`.
/// * `dst` - Destination directory inside the container rootfs.
/// * `output_layout_name` - Layout whose rootfs is the copy target.
/// * `build_dir` - Absolute path of the directory the Crabtainerfile lives in.
///
/// # Errors
///
/// Returns an error string for invalid glob patterns, missing parent dirs,
/// and failed copies. Patterns matching nothing log a warning instead.
pub async fn copy_to_layout(
    src: &str,
    dst: &str,
    output_layout_name: &str,
    build_dir: &PathBuf,
) -> Result<(), String> {
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
    if fs::metadata(build_dir.join(".crabtainerignore")).is_ok() {
        let splited: Vec<String> = fs::read_to_string(build_dir.join(".crabtainerignore"))
            .expect(" => [COPY] Failed to open .crabtainerignore")
            .split("\n")
            .map(|s| s.to_string())
            .collect();

        ignore.extend(splited);
    }

    let ignore_engine = CrabtainerIgnore::new(build_dir);

    // Case 1: Universal wildcard (Copy entire workspace respecting ignores)
    if src == "*" {
        let files_to_copy = ignore_engine.collect_files(build_dir.as_path());

        for src_path in files_to_copy {
            let rel_path = src_path
                .strip_prefix(build_dir)
                .expect("Failed to strip prefix");
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
        let conjoin_path = PathBuf::from(format!(
            "{}/{}",
            build_dir
                .to_str()
                .expect("Failed to convert build_dir to &str"),
            src_path.display()
        ));
        // Skip ignored paths
        if ignore_engine.is_ignored(src_path.as_path()) {
            println!(" => [COPY] Skipping ignored path: {}", src_path.display());
            continue;
        }

        if src_path.is_dir() {
            // Recursively collect and copy directory contents
            let files = ignore_engine.collect_files(conjoin_path.as_path());
            for absolute_file in files {
                let rel_file = absolute_file
                    .strip_prefix(build_dir)
                    .expect("Failed to strip prefix");
                let target_path = destination.join(rel_file);
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!(
                            " => [COPY] Failed to create dir {}: {}",
                            parent.display(),
                            e
                        )
                    })?;
                }
                fs::copy(rel_file, &target_path).map_err(|e| {
                    format!(" => [COPY] Failed to copy {}: {}", rel_file.display(), e)
                })?;
            }
        } else if conjoin_path.is_file() {
            // Replicate relative structure under destination
            let rel_path = conjoin_path
                .strip_prefix(build_dir)
                .expect("Failed to strip prefix");
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

            fs::copy(&conjoin_path, &target_path)
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

    /// Creates `rel` (a relative path) under `root` with "content" inside.
    fn write_file(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "content").unwrap();
    }

    /// Writes `rules` to `<root>/.crabtainerignore` and returns the compiled
    /// [`CrabtainerIgnore`].
    fn ignore_with(root: &Path, rules: &str) -> CrabtainerIgnore {
        fs::write(root.join(".crabtainerignore"), rules).unwrap();
        CrabtainerIgnore::new(root)
    }

    /// Collects the files of `ig` under `root`, sorted and relativized to
    /// `root` for easy comparison.
    fn collected(root: &Path, ig: &CrabtainerIgnore) -> Vec<PathBuf> {
        let mut files = ig.collect_files(root);
        files.sort();
        files
            .into_iter()
            .map(|f| {
                f.strip_prefix(root)
                    .expect("Failed to strip prefix")
                    .to_path_buf()
            })
            .collect()
    }

    #[test]
    /// Without an ignore file every file is collected.
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
    /// The `.crabtainerignore` file itself is always excluded.
    fn crabtainerignore_itself_excluded_even_without_rules() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        fs::write(dir.path().join(".crabtainerignore"), "").unwrap();

        let ig = CrabtainerIgnore::new(dir.path());
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("a.txt")]);
    }

    #[test]
    /// A file glob like `*.txt` matches at any depth.
    fn file_pattern_ignores_at_any_depth() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "b.log");
        write_file(dir.path(), "sub/nested.txt");

        let ig = ignore_with(dir.path(), "*.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("b.log")]);
    }

    #[test]
    /// A plain directory rule prunes the whole subtree.
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
    /// A trailing slash still makes the rule target a directory.
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
    /// A leading slash anchors the rule to the ignore root.
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
    /// Wildcard patterns ignore every matching file.
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
    /// `!` rules re-include a specific file.
    fn negation_reincludes_specific_file() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "keep.txt");
        write_file(dir.path(), "drop.txt");

        let ig = ignore_with(dir.path(), "*.txt\n!keep.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("keep.txt")]);
    }

    #[test]
    /// A later negation overrides an earlier ignore rule.
    fn later_negation_overrides_previous_rule() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "keep.txt");

        let ig = ignore_with(dir.path(), "*\n!keep.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("keep.txt")]);
    }

    #[test]
    /// A later ignore rule overrides an earlier negation.
    fn later_rule_overrides_previous_negation() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "keep.txt");

        let ig = ignore_with(dir.path(), "!keep.txt\n*\n");
        assert_eq!(collected(dir.path(), &ig), Vec::<PathBuf>::new());
    }

    #[test]
    /// A negated directory rule re-includes its contents.
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
    /// Comments and blank lines in the ignore file are skipped.
    fn comments_and_blank_lines_are_skipped() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "a.txt");
        write_file(dir.path(), "b.log");

        let ig = ignore_with(dir.path(), "# comment\n\n*.txt\n");
        assert_eq!(collected(dir.path(), &ig), vec![PathBuf::from("b.log")]);
    }
}
