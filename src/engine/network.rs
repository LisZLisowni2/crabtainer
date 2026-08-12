use std::net::Ipv4Addr;
use rtnetlink::{new_connection, Error, Handle, LinkVeth};
use futures::stream::TryStreamExt;
use nix::unistd::Pid;

pub async fn create_bridge(container_pid: Pid) -> Result<(), Box<dyn std::error::Error>> {
    let (connection, handle, _) = new_connection()?;
    tokio::spawn(connection);

    let bridge_name = "rustocker0";

    println!("[NETWORK] Connected with RTNetLink");
    create_veth_pair(&handle, "veth1", "veth1-peer").await?;

    Ok(())
}

pub async fn create_veth_pair(handle: &Handle, host_dev: &str, cont_dev: &str) -> Result<(), Error> {
    let request = handle
        .link()
        .add(LinkVeth::new(host_dev, cont_dev).build());

    request.execute().await
}
