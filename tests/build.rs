mod common;

use rustocker::engine::builder::build_layout;
use rustocker::engine::container::{ContainerOptions, run_container};
use rustocker::engine::instructions::copy::copy_to_layout;
use rustocker::engine::instructions::from::from_image;

#[tokio::test]
async fn copy_to_layout_copies_a_single_file() {
    let _env = common::isolated_home();
    let home = _env.home();

    copy_to_layout("Cargo.toml", "/app", "my-layout")
        .await
        .unwrap();

    let copied = home
        .join("layouts")
        .join("my-layout")
        .join("rootfs")
        .join("app")
        .join("Cargo.toml");
    assert!(copied.is_file(), "file should be copied to {:?}", copied);
    assert_eq!(
        std::fs::read_to_string(&copied).unwrap(),
        std::fs::read_to_string("Cargo.toml").unwrap()
    );
}

#[tokio::test]
async fn copy_to_layout_copies_directory_recursively() {
    let _env = common::isolated_home();
    let home = _env.home();

    copy_to_layout("src", "/code", "my-layout").await.unwrap();

    let copied = home
        .join("layouts")
        .join("my-layout")
        .join("rootfs")
        .join("code")
        .join("src")
        .join("lib.rs");
    assert!(
        copied.is_file(),
        "directory contents should be copied to {:?}",
        copied
    );
}

#[tokio::test]
async fn copy_to_layout_expands_glob_patterns() {
    let _env = common::isolated_home();
    let home = _env.home();

    copy_to_layout("Cargo.*", "/pkgs", "my-layout")
        .await
        .unwrap();

    let rootfs = home
        .join("layouts")
        .join("my-layout")
        .join("rootfs")
        .join("pkgs");
    assert!(rootfs.join("Cargo.toml").is_file());
    assert!(rootfs.join("Cargo.lock").is_file());
}

#[tokio::test]
async fn copy_to_layout_star_respects_rustockerignore() {
    let _env = common::isolated_home();
    let home = _env.home();

    copy_to_layout("*", "/workspace", "my-layout")
        .await
        .unwrap();

    let rootfs = home
        .join("layouts")
        .join("my-layout")
        .join("rootfs")
        .join("workspace");
    assert!(rootfs.join("Cargo.toml").is_file());
    assert!(rootfs.join("src").join("lib.rs").is_file());
    assert!(
        !rootfs.join("target").exists(),
        "ignored directory should not be copied"
    );
    assert!(
        !rootfs.join(".rustockerignore").exists(),
        "ignore file itself is never copied"
    );
}

#[tokio::test]
async fn from_image_errors_when_image_missing() {
    let _env = common::isolated_home();
    let home = _env.home();
    std::fs::create_dir_all(home.join("images")).unwrap();

    let err = from_image(&"nonexistent".to_string(), &"out".to_string())
        .await
        .unwrap_err();
    assert!(
        err.contains("can not be found"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn from_image_extracts_rootfs_into_layout() {
    let _env = common::isolated_home();
    let home = _env.home();
    common::create_tarball(
        home,
        "base",
        &[("etc/hello.txt", "hi"), ("bin/tool", "tool")],
    );

    from_image(&"base".to_string(), &"out".to_string())
        .await
        .unwrap();

    let rootfs = home.join("layouts").join("out").join("rootfs");
    assert_eq!(
        std::fs::read_to_string(rootfs.join("etc/hello.txt")).unwrap(),
        "hi"
    );
    assert_eq!(
        std::fs::read_to_string(rootfs.join("bin/tool")).unwrap(),
        "tool"
    );
}

#[tokio::test]
async fn build_layout_processes_instructions_end_to_end() {
    let _env = common::isolated_home();
    let home = _env.home();
    common::create_tarball(home, "base", &[("marker.txt", "rootfs-marker")]);

    let rustockerfile = home.join("Rustockerfile");
    std::fs::write(&rustockerfile, "FROM base\nCOPY . /opt/app\n").unwrap();

    build_layout(
        rustockerfile.to_str().unwrap().to_string(),
        "final".to_string(),
    )
    .await
    .unwrap();

    let rootfs = home.join("layouts").join("final").join("rootfs");
    assert!(
        rootfs.join("marker.txt").is_file(),
        "base image should be extracted"
    );
    assert!(
        rootfs.join("opt").join("app").is_dir(),
        "COPY target should exist"
    );

    let config = home.join("layouts").join("final").join("config.json");
    assert!(config.is_file(), "config.json should be written");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(
        json["cmd"],
        serde_json::Value::Null,
        "no CMD instruction -> cmd is null"
    );
    assert_eq!(
        json["args"],
        serde_json::json!([]),
        "no CMD instruction -> empty args"
    );
    assert!(
        json.get("rootfs").is_none(),
        "rootfs field was removed from config"
    );
}

#[tokio::test]
async fn build_layout_injects_cmd_into_config_json() {
    let _env = common::isolated_home();
    let home = _env.home();
    common::create_tarball(home, "base", &[("marker.txt", "rootfs-marker")]);

    let rustockerfile = home.join("Rustockerfile");
    std::fs::write(&rustockerfile, "FROM base\nCMD /bin/sh -c\n").unwrap();

    build_layout(
        rustockerfile.to_str().unwrap().to_string(),
        "final".to_string(),
    )
    .await
    .unwrap();

    let config = home.join("layouts").join("final").join("config.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(json["cmd"], serde_json::json!("/bin/sh"));
    assert_eq!(json["args"], serde_json::json!(["-c"]));
}

#[tokio::test]
async fn build_layout_propagates_parse_errors() {
    let _env = common::isolated_home();
    let home = _env.home();

    let rustockerfile = home.join("Rustockerfile");
    std::fs::write(&rustockerfile, "NOT_A_KEYWORD foo\n").unwrap();

    let err = build_layout(
        rustockerfile.to_str().unwrap().to_string(),
        "final".to_string(),
    )
    .await
    .unwrap_err();
    assert!(err.contains("Unknown keyword"), "unexpected error: {}", err);
}

#[tokio::test]
async fn run_container_errors_when_layout_missing() {
    let _env = common::isolated_home();

    let opts = ContainerOptions {
        layout_name: "does-not-exist".to_string(),
        command: "/bin/true".to_string(),
        args: vec![],
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

    let err = result.unwrap_err();
    assert!(err.contains("doesn't exist"), "unexpected error: {}", err);
}
