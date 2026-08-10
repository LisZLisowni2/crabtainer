use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::engine::container::ContainerOptions;
use tokio::io::AsyncWriteExt;

pub struct ResourcesLimits {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<u64>
}

pub async fn setup_cgroups(container_id: &String, opts: &ContainerOptions) -> Result<PathBuf, String> {
    let cgroup_dir = PathBuf::from("/sys/fs/cgroup").join(container_id);

    std::fs::create_dir_all(&cgroup_dir)
        .map_err(|e| format!("[CGROUP] Failed to create cgroup dir: {}", e))?;

    if let Some(memory) = opts.memory_limit {
        let bytes = memory as u64;
        std::fs::write(cgroup_dir.join("memory.max"), bytes.to_string())
            .map_err(|e| format!("[CGROUP] Failed to write memory: {}", e))?;
    }

    if let Some(quota) = opts.cpu_limit {
        let period = 100_000;
        let cpu_max_val = format!("{}, {}", quota, period);
        println!("[CGROUP] CPU max value: {}", cpu_max_val);

        std::fs::write(cgroup_dir.join("cpu.max"), cpu_max_val)
            .map_err(|e| format!("[CGROUP]  Failed to write cpu limit: {}", e))?;
    }

    Ok(cgroup_dir)
}

pub async fn attach_process_to_cgroup(cgroup_dir: &Path, pid: nix::unistd::Pid) -> Result<(), String> {
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

    file.write_all(pid_vec).map_err(|e| format!("[CGROUP] Failed to write cgroup file: {}", e))?;

    Ok(())
}