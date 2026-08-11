//! Privileged integration tests.
//!
//! These exercise the namespace/mount code paths (`clone`, `chroot`, OverlayFS)
//! and can only run as root on a machine with the required kernel features.
//! They are ignored by default. Run them manually with:
//!
//! ```text
//! sudo cargo test --test privileged -- --ignored --nocapture
//! ```
//!
//! The container test builds a minimal rootfs from a statically-linked shell
//! (e.g. busybox). If none is found the test skips with a message instead of
//! failing.

mod common;

use std::path::{Path, PathBuf};

use rustocker::engine::container::{ContainerOptions, run_container};
use rustocker::engine::instructions::from::from_image;
use rustocker::engine::instructions::run::run_in_container;
use rustocker::engine::paths::RustockerPaths;

fn find_static_shell() -> Option<PathBuf> {
    let candidates = [
        "/bin/busybox",
        "/usr/bin/busybox",
        "/sbin/busybox",
        "/bin/sh",
        "/usr/bin/sh",
    ];

    for candidate in candidates {
        let path = Path::new(candidate);
        if !path.exists() {
            continue;
        }

        let ldd = std::process::Command::new("ldd").arg(path).output().ok()?;
        let out = format!(
            "{}{}",
            String::from_utf8_lossy(&ldd.stdout),
            String::from_utf8_lossy(&ldd.stderr)
        );
        if out.contains("not a dynamic executable") {
            return Some(path.to_path_buf());
        }
    }

    None
}

fn install_rootfs_shell(rootfs: &Path, shell: &Path) {
    let bytes = std::fs::read(shell).unwrap();

    let bin = rootfs.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("sh"), &bytes).unwrap();

    if shell.file_name().unwrap_or_default() == "busybox" {
        std::fs::write(bin.join("busybox"), bytes).unwrap();
    }

    use std::os::unix::fs::PermissionsExt;
    for entry in std::fs::read_dir(&bin).unwrap().flatten() {
        std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[tokio::test]
#[ignore = "requires root + kernel namespaces; run with sudo cargo test --test privileged -- --ignored"]
async fn run_in_container_executes_command_in_namespace() {
    if !is_root::is_root() {
        eprintln!("SKIP: requires root privileges");
        return;
    }

    let _env = common::isolated_home();
    let home = _env.home();

    let rootfs = RustockerPaths::layout_store_dir()
        .join("layout-a")
        .join("rootfs");
    std::fs::create_dir_all(&rootfs).unwrap();

    let shell = match find_static_shell() {
        Some(shell) => shell,
        None => {
            eprintln!("SKIP: no statically-linked shell (e.g. busybox) found on this system");
            return;
        }
    };
    install_rootfs_shell(&rootfs, &shell);

    let result = run_in_container(
        &"layout-a".to_string(),
        "echo rustocker > /proof.txt".to_string(),
    )
    .await;
    assert!(result.is_ok(), "run_in_container failed: {:?}", result);

    let proof = rootfs.join("proof.txt");
    assert!(proof.exists(), "command output not found in rootfs");
    assert_eq!(std::fs::read_to_string(proof).unwrap().trim(), "rustocker");
    let _ = home;
}

#[tokio::test]
#[ignore = "requires root + kernel namespaces; run with sudo cargo test --test privileged -- --ignored"]
async fn run_container_requires_root_and_overlayfs() {
    if !is_root::is_root() {
        eprintln!("SKIP: requires root privileges");
        return;
    }

    let _env = common::isolated_home();
    let home = _env.home();

    common::create_tarball(home, "base", &[("marker", "1")]);
    from_image(&"base".to_string(), &"layout-a".to_string())
        .await
        .unwrap();

    let layout_config = home.join("layouts").join("layout-a").join("config.json");
    let config = rustocker::engine::builder::LayoutOpts {
        cpu_limit: None,
        memory_limit: None,
        args: vec!["/bin/sh".to_string(), "-c".to_string(), "true".to_string()],
    };
    std::fs::write(&layout_config, serde_json::to_string(&config).unwrap()).unwrap();

    let opts = ContainerOptions {
        layout_name: "layout-a".to_string(),
        args: vec!["/bin/sh".to_string(), "-c".to_string(), "true".to_string()],
        cpu_limit: None,
        memory_limit: None,
    };
    let result = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(run_container(opts))
        })
        .unwrap()
        .join()
        .unwrap();

    if let Err(e) = result {
        if e.contains("Error during mounting OverlayFS") || e.contains("Operation not permitted") {
            eprintln!(
                "SKIP: OverlayFS mount not permitted in this environment: {}",
                e
            );
        } else {
            panic!("unexpected error: {}", e);
        }
    }
}
