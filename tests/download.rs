mod common;

use httpmock::prelude::*;
use Rustocker::engine::instructions::download::download_image_if_missing;

#[tokio::test]
async fn downloads_image_when_missing() {
    let _env = common::isolated_home();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/alpine.tar.gz");
        then.status(200)
            .header("content-type", "application/gzip")
            .body("fake-tarball-bytes");
    });

    let home = _env.home();
    let path = download_image_if_missing(&server.url("/alpine.tar.gz"), "alpine")
        .await
        .unwrap();

    mock.assert();
    assert_eq!(path, home.join("images").join("alpine.tar.gz"));
    assert_eq!(std::fs::read(&path).unwrap(), b"fake-tarball-bytes");
}

#[tokio::test]
async fn skips_download_when_image_exists() {
    let _env = common::isolated_home();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/existing.tar.gz");
        then.status(200).body("should-not-be-fetched");
    });

    let home = _env.home();
    let img_dir = home.join("images");
    std::fs::create_dir_all(&img_dir).unwrap();
    std::fs::write(img_dir.join("existing.tar.gz"), "existing").unwrap();

    let path = download_image_if_missing(&server.url("/existing.tar.gz"), "existing")
        .await
        .unwrap();

    assert_eq!(path, img_dir.join("existing.tar.gz"));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    mock.assert_hits(0);
}

#[tokio::test]
async fn propagates_http_errors() {
    let _env = common::isolated_home();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/missing.tar.gz");
        then.status(404);
    });

    let err = download_image_if_missing(&server.url("/missing.tar.gz"), "missing")
        .await
        .unwrap_err();

    assert!(err.contains("Error downloading image"), "unexpected error: {}", err);
    mock.assert();
}
