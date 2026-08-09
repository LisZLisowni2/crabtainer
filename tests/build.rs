mod common;

use Rustocker::engine::builder::build_layout;
use Rustocker::engine::container::{run_container, ContainerOptions};
use Rustocker::engine::instructions::copy::{copy_to_layout};
use Rustocker::engine::instructions::from::from_image;

#[tokio::test]
async fn copy_creates_nested_destination_directories() {
    let _env = common::isolated_home();
    let home = _env.home();

    copy_to_layout("whatever", "/etc/app/config", &"my-layout".to_string())
        .await
        .unwrap();

    let expected = home.join("layouts").join("my-layout").join("rootfs").join("etc").join("app").join("config");
    assert!(expected.is_dir());
}

#[tokio::test]
async fn copy_handles_relative_destination() {
    let _env = common::isolated_home();
    let home = _env.home();

    copy_to_layout("whatever", "opt/bin", &"my-layout")
        .await
        .unwrap();

    let expected = home.join("layouts").join("my-layout").join("rootfs").join("opt").join("bin");
    assert!(expected.is_dir());
}

#[tokio::test]
async fn from_image_errors_when_image_missing() {
    let _env = common::isolated_home();
    let home = _env.home();
    std::fs::create_dir_all(home.join("images")).unwrap();

    let err = from_image(&"nonexistent".to_string(), &"out".to_string()).await.unwrap_err();
    assert!(err.contains("can not be found"), "unexpected error: {}", err);
}

#[tokio::test]
async fn from_image_extracts_rootfs_into_layout() {
    let _env = common::isolated_home();
    let home = _env.home();
    common::create_tarball(home, "base", &[("etc/hello.txt", "hi"), ("bin/tool", "tool")]);

    from_image(&"base".to_string(), &"out".to_string()).await.unwrap();

    let rootfs = home.join("layouts").join("out").join("rootfs");
    assert_eq!(std::fs::read_to_string(rootfs.join("etc/hello.txt")).unwrap(), "hi");
    assert_eq!(std::fs::read_to_string(rootfs.join("bin/tool")).unwrap(), "tool");
}

#[tokio::test]
async fn build_layout_processes_instructions_end_to_end() {
    let _env = common::isolated_home();
    let home = _env.home();
    common::create_tarball(home, "base", &[("marker.txt", "rootfs-marker")]);

    let rustockerfile = home.join("Rustockerfile");
    std::fs::write(&rustockerfile, "FROM base\nCOPY . /opt/app\n").unwrap();

    build_layout(rustockerfile.to_str().unwrap().to_string(), "final".to_string())
        .await
        .unwrap();

    let rootfs = home.join("layouts").join("final").join("rootfs");
    assert!(rootfs.join("marker.txt").is_file(), "base image should be extracted");
    assert!(rootfs.join("opt").join("app").is_dir(), "COPY target should exist");
}

#[tokio::test]
async fn build_layout_propagates_parse_errors() {
    let _env = common::isolated_home();
    let home = _env.home();

    let rustockerfile = home.join("Rustockerfile");
    std::fs::write(&rustockerfile, "NOT_A_KEYWORD foo\n").unwrap();

    let err = build_layout(rustockerfile.to_str().unwrap().to_string(), "final".to_string())
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
    };

    let result = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || run_container(opts))
        .unwrap()
        .join()
        .unwrap();

    let err = result.unwrap_err();
    assert!(err.contains("doesn't exist"), "unexpected error: {}", err);
}
