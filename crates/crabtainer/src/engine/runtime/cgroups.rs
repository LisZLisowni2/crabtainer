//! cgroup v2 resource control.
//!
//! Creates a per-container cgroup under the v2 hierarchy, writes CPU and
//! memory limits, and attaches the container's root process to it.

use crate::engine::runtime::options::ContainerReady;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Sets up the container's cgroup under the standard v2 mount point
/// (`/sys/fs/cgroup`).
///
/// Delegates to the private helper `setup_cgroups_in` — see there for
/// details.
///
/// # Errors
///
/// Returns an error string if the cgroup directory or any limit file cannot
/// be created or written.
pub fn setup_cgroups(container_id: &str, opts: &ContainerReady) -> Result<PathBuf, String> {
    setup_cgroups_in(container_id, opts, Path::new("/sys/fs/cgroup"))
}

/// Creates the cgroup directory `<base>/<container_id>` and writes the
/// requested limits into it.
///
/// - `memory.max` is written when `opts.memory_limit` is set.
/// - `cpu.max` is written as `<quota>, 100000` when `opts.quota` is set.
///
/// # Arguments
///
/// * `container_id` - Container id used as the cgroup directory name.
/// * `opts` - Resolved limits from [`ContainerReady`].
/// * `base_dir` - Root of the cgroup hierarchy (injectable for tests).
///
/// # Errors
///
/// Returns an error string if the directory or any limit file cannot be
/// written.
fn setup_cgroups_in(
    container_id: &str,
    opts: &ContainerReady,
    base_dir: &Path,
) -> Result<PathBuf, String> {
    let cgroup_dir = base_dir.join(container_id);

    std::fs::create_dir_all(&cgroup_dir)
        .map_err(|e| format!("[CGROUP] Failed to create cgroup dir: {}", e))?;

    if let Some(memory) = opts.memory_limit {
        let bytes = memory as u64;
        std::fs::write(cgroup_dir.join("memory.max"), bytes.to_string())
            .map_err(|e| format!("[CGROUP] Failed to write memory: {}", e))?;
    }

    if let Some(quota) = opts.quota {
        let period = 100_000u64;
        let cpu_max_val = format!("{}, {}", quota, period);

        std::fs::write(cgroup_dir.join("cpu.max"), cpu_max_val)
            .map_err(|e| format!("[CGROUP]  Failed to write cpu limit: {}", e))?;
    }

    Ok(cgroup_dir)
}

/// Attaches a process to the container's cgroup by writing its PID to
/// `cgroup.procs`.
///
/// # Arguments
///
/// * `cgroup_dir` - Path of the prepared cgroup directory.
/// * `pid` - PID of the process to attach.
///
/// # Errors
///
/// Returns an error string for invalid PIDs (`<= 0`) or if the
/// `cgroup.procs` file cannot be opened or written.
pub fn attach_process_to_cgroup(cgroup_dir: &Path, pid: nix::unistd::Pid) -> Result<(), String> {
    let procs_file = cgroup_dir.join("cgroup.procs");

    if pid.as_raw() <= 0 {
        return Err(format!("[CGROUP] Invalid pid: {}", pid));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(&procs_file)
        .map_err(|e| format!("[CGROUP] Failed to open cgroup file: {}", e))?;

    let pid_str = pid.to_string();
    let pid_vec: &[u8] = pid_str.as_bytes();

    file.write_all(pid_vec)
        .map_err(|e| format!("[CGROUP] Failed to write cgroup file: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::runtime::options::RestartPolicy;
    use tempfile::tempdir;

    /// Builds a [`ContainerReady`] with the given CPU quota and memory limit.
    fn opts(cpu: Option<i64>, mem: Option<i64>) -> ContainerReady {
        ContainerReady {
            layout_name: "test".to_string(),
            args: vec!["true".to_string()],
            quota: cpu,
            memory_limit: mem,
            restart_policy: RestartPolicy::Never,
            workdir: "/".to_string(),
            envs: vec![],
        }
    }

    #[tokio::test]
    /// `setup_cgroups_in` writes memory and cpu limit files.
    async fn setup_writes_memory_and_cpu_limits() {
        let dir = tempdir().unwrap();
        let result =
            setup_cgroups_in("cgroup-test", &opts(Some(150000), Some(2048)), dir.path()).unwrap();
        assert_eq!(result, dir.path().join("cgroup-test"));
        assert_eq!(
            std::fs::read_to_string(result.join("memory.max")).unwrap(),
            "2048"
        );
        assert_eq!(
            std::fs::read_to_string(result.join("cpu.max")).unwrap(),
            "150000, 100000"
        );
    }

    #[tokio::test]
    /// Without limits only the cgroup directory is created.
    async fn setup_with_no_limits_only_creates_dir() {
        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("empty-cgroup", &opts(None, None), dir.path()).unwrap();
        assert!(result.is_dir());
        assert!(!result.join("memory.max").exists());
        assert!(!result.join("cpu.max").exists());
    }

    #[tokio::test]
    /// Only the limits that are actually provided get written.
    async fn setup_writes_only_provided_limits() {
        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("cpu-only", &opts(Some(200000), None), dir.path()).unwrap();
        assert!(!result.join("memory.max").exists());
        assert_eq!(
            std::fs::read_to_string(result.join("cpu.max")).unwrap(),
            "200000, 100000"
        );

        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("mem-only", &opts(None, Some(512)), dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(result.join("memory.max")).unwrap(),
            "512"
        );
        assert!(!result.join("cpu.max").exists());
    }

    #[tokio::test]
    /// Attaching a non-positive PID is rejected.
    async fn attach_rejects_invalid_pid() {
        let dir = tempdir().unwrap();
        let err = attach_process_to_cgroup(dir.path(), nix::unistd::Pid::from_raw(0)).unwrap_err();
        assert!(err.contains("Invalid pid"), "unexpected error: {}", err);
    }

    #[tokio::test]
    /// `attach_process_to_cgroup` writes the PID into `cgroup.procs`.
    async fn attach_writes_pid_to_cgroup_procs() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("cgroup.procs"), "").unwrap();
        attach_process_to_cgroup(dir.path(), nix::unistd::Pid::from_raw(1234)).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("cgroup.procs")).unwrap(),
            "1234"
        );
    }

    #[tokio::test]
    /// Attaching to a directory without `cgroup.procs` reports the error.
    async fn attach_reports_missing_procs_file() {
        let dir = tempdir().unwrap();
        let err = attach_process_to_cgroup(dir.path(), nix::unistd::Pid::from_raw(1)).unwrap_err();
        assert!(
            err.contains("Failed to open cgroup file"),
            "unexpected error: {}",
            err
        );
    }
}
