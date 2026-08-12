use crate::engine::container::{ContainerOptions, ContainerReady};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct ResourcesLimits {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<u64>,
}

pub async fn setup_cgroups(container_id: &str, opts: &ContainerReady) -> Result<PathBuf, String> {
    setup_cgroups_in(container_id, opts, Path::new("/sys/fs/cgroup")).await
}

async fn setup_cgroups_in(
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

pub async fn attach_process_to_cgroup(
    cgroup_dir: &Path,
    pid: nix::unistd::Pid,
) -> Result<(), String> {
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
    use tempfile::tempdir;

    fn opts(cpu: Option<i64>, mem: Option<i64>) -> ContainerReady {
        ContainerReady {
            layout_name: "test".to_string(),
            args: vec!["true".to_string()],
            quota: cpu,
            memory_limit: mem,
        }
    }

    #[tokio::test]
    async fn setup_writes_memory_and_cpu_limits() {
        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("cgroup-test", &opts(Some(150000), Some(2048)), dir.path())
            .await
            .unwrap();
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
    async fn setup_with_no_limits_only_creates_dir() {
        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("empty-cgroup", &opts(None, None), dir.path())
            .await
            .unwrap();
        assert!(result.is_dir());
        assert!(!result.join("memory.max").exists());
        assert!(!result.join("cpu.max").exists());
    }

    #[tokio::test]
    async fn setup_writes_only_provided_limits() {
        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("cpu-only", &opts(Some(200000), None), dir.path())
            .await
            .unwrap();
        assert!(!result.join("memory.max").exists());
        assert_eq!(
            std::fs::read_to_string(result.join("cpu.max")).unwrap(),
            "200000, 100000"
        );

        let dir = tempdir().unwrap();
        let result = setup_cgroups_in("mem-only", &opts(None, Some(512)), dir.path())
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(result.join("memory.max")).unwrap(),
            "512"
        );
        assert!(!result.join("cpu.max").exists());
    }

    #[tokio::test]
    async fn attach_rejects_invalid_pid() {
        let dir = tempdir().unwrap();
        let err = attach_process_to_cgroup(dir.path(), nix::unistd::Pid::from_raw(0))
            .await
            .unwrap_err();
        assert!(err.contains("Invalid pid"), "unexpected error: {}", err);
    }

    #[tokio::test]
    async fn attach_writes_pid_to_cgroup_procs() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("cgroup.procs"), "").unwrap();
        attach_process_to_cgroup(dir.path(), nix::unistd::Pid::from_raw(1234))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("cgroup.procs")).unwrap(),
            "1234"
        );
    }

    #[tokio::test]
    async fn attach_reports_missing_procs_file() {
        let dir = tempdir().unwrap();
        let err = attach_process_to_cgroup(dir.path(), nix::unistd::Pid::from_raw(1))
            .await
            .unwrap_err();
        assert!(
            err.contains("Failed to open cgroup file"),
            "unexpected error: {}",
            err
        );
    }
}
