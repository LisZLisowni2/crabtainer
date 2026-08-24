use oci_spec::runtime::{
    LinuxBuilder, LinuxCpuBuilder, LinuxMemoryBuilder, LinuxNamespaceBuilder, LinuxNamespaceType,
    LinuxResourcesBuilder, ProcessBuilder, RootBuilder, SpecBuilder,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct LayoutOpts {
    pub memory_limit: Option<f64>,
    pub cpu_limit: Option<f64>,
    pub args: Vec<String>,
}

pub async fn save_config(
    opts: LayoutOpts,
    layout_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let period = 100000.0;
    let quota = if let Some(cpu) = opts.cpu_limit {
        cpu * period
    } else {
        period
    };

    let memory = opts.memory_limit.unwrap_or(1024f64 * 1024f64 * 1024f64); // 1GB Default

    let spec = SpecBuilder::default()
        .root(
            RootBuilder::default()
                .path("rootfs")
                .readonly(false)
                .build()?,
        )
        .process(
            ProcessBuilder::default()
                .terminal(true)
                .args(opts.args)
                .build()?,
        )
        .linux(
            LinuxBuilder::default()
                .namespaces(vec![
                    LinuxNamespaceBuilder::default()
                        .typ(LinuxNamespaceType::Pid)
                        .build()?,
                    LinuxNamespaceBuilder::default()
                        .typ(LinuxNamespaceType::Mount)
                        .build()?,
                    LinuxNamespaceBuilder::default()
                        .typ(LinuxNamespaceType::Uts)
                        .build()?,
                    LinuxNamespaceBuilder::default()
                        .typ(LinuxNamespaceType::Ipc)
                        .build()?,
                    LinuxNamespaceBuilder::default()
                        .typ(LinuxNamespaceType::Network)
                        .build()?,
                ])
                .resources(
                    LinuxResourcesBuilder::default()
                        .cpu(
                            LinuxCpuBuilder::default()
                                .quota(quota as i64)
                                .period(period as u64)
                                .build()?,
                        )
                        .memory(LinuxMemoryBuilder::default().limit(memory as i64).build()?)
                        .build()?,
                )
                .build()?,
        )
        .build()?;

    let config_path = layout_path.join("config.json");

    let json_data = serde_json::to_string_pretty(&spec)?;

    tokio::fs::write(config_path, json_data).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oci_spec::runtime::Spec;

    fn sample_opts() -> LayoutOpts {
        LayoutOpts {
            memory_limit: Some(2048.0),
            cpu_limit: Some(1.5),
            args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "echo hi".to_string(),
            ],
        }
    }

    #[test]
    fn layout_opts_serializes_round_trip() {
        let opts = sample_opts();
        let json = serde_json::to_string(&opts).unwrap();
        let decoded: LayoutOpts = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.memory_limit, Some(2048.0));
        assert_eq!(decoded.cpu_limit, Some(1.5));
        assert_eq!(
            decoded.args,
            vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "echo hi".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn save_config_writes_limits_and_args() {
        let dir = tempfile::tempdir().unwrap();
        save_config(sample_opts(), dir.path().to_path_buf())
            .await
            .unwrap();

        let spec = Spec::load(dir.path().join("config.json")).unwrap();
        let resources = spec.linux().as_ref().unwrap().resources().as_ref().unwrap();

        assert_eq!(resources.cpu().as_ref().unwrap().quota(), Some(150000));
        assert_eq!(resources.cpu().as_ref().unwrap().period(), Some(100000));
        assert_eq!(resources.memory().as_ref().unwrap().limit(), Some(2048));

        assert_eq!(spec.root().as_ref().unwrap().path(), "rootfs");
        assert_eq!(
            spec.process().as_ref().unwrap().args().as_ref().unwrap(),
            &vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "echo hi".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn save_config_uses_defaults_without_limits() {
        let dir = tempfile::tempdir().unwrap();
        let opts = LayoutOpts {
            memory_limit: None,
            cpu_limit: None,
            args: vec![],
        };
        save_config(opts, dir.path().to_path_buf()).await.unwrap();

        let spec = Spec::load(dir.path().join("config.json")).unwrap();
        let resources = spec.linux().as_ref().unwrap().resources().as_ref().unwrap();

        assert_eq!(resources.cpu().as_ref().unwrap().quota(), Some(100000));
        assert_eq!(resources.cpu().as_ref().unwrap().period(), Some(100000));
        assert_eq!(resources.memory().as_ref().unwrap().limit(), Some(i64::MAX));
        assert!(
            spec.process()
                .as_ref()
                .unwrap()
                .args()
                .as_ref()
                .unwrap()
                .is_empty()
        );
    }
}
