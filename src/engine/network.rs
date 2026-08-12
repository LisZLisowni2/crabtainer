use std::net::Ipv4Addr;
use rtnetlink::{new_connection, Error, Handle, LinkVeth, LinkBridge, LinkMessageBuilder, LinkUnspec};
use futures::stream::TryStreamExt;
use netlink_packet_route::link::LinkMessage;
use nix::unistd::Pid;

pub struct NetworkManager {
    handle: Handle,
    bridge_name: String,
    gateway_ip: Ipv4Addr,
    subnet_prefix: u8
}

impl NetworkManager {
    pub async fn new(bridge_name: String, gateway_ip: Ipv4Addr, subnet_prefix: u8) -> Result<Self, Box<dyn std::error::Error>> {
        let (connection, handle, _) = new_connection().await?;
        tokio::spawn(connection);

        Ok(Self {
            handle,
            bridge_name,
            gateway_ip,
            subnet_prefix
        })
    }

    pub async fn init_global_network(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(_) = self.get_link_index(self.bridge_name.as_str()).await {
            println!("[NETWORK WARN] Global network currently initialized. Skipping");
            return Ok(());
        }

        println!("[NETWORK] Creating new global network");
        self.handle
            .link()
            .add(LinkBridge::new(self.bridge_name.as_str()).build())
            .execute()
            .await?;

        println!("[NETWORK] Created new bridge: {}", self.bridge_name);

        let bridge_idx = self.get_link_index(self.bridge_name.as_str()).await?;

        self.handle
            .address()
            .add(bridge_idx, std::net::IpAddr::V4(self.gateway_ip), self.subnet_prefix)
            .execute()
            .await?;

        println!("[NETWORK] Set IP for bridge: {}", self.bridge_name);

        self.handle
            .link()
            .set(LinkMessageBuilder::<LinkUnspec>::default()
                .index(bridge_idx)
                .up()
                .build()
            ).execute().await?;
        println!("[NETWORK] Set UP bridge: {}", self.bridge_name);

        Ok(())
    }

    pub async fn attach_container(&self, container_id: &str, container_pid: u32) -> Result<(), Box<dyn std::error::Error>> {
        let short_id = &container_id[..6.min(container_id.len())];
        let veth_host = format!("veth_{}", short_id);
        let veth_cont = format!("vethc_{}", short_id);

        println!("[NETWORK] Creating new pair link for container {}: {} <-> {}", container_id, veth_host, veth_cont);

        self.handle
            .link()
            .add(
                LinkVeth::new(veth_host.as_str(), veth_cont.as_str()).build()
            ).execute().await?;

        let bridge_id = self.get_link_index(self.bridge_name.as_str()).await?;
        let veth_host_id = self.get_link_index(veth_host.as_str()).await?;
        let veth_cont_id = self.get_link_index(veth_cont.as_str()).await?;

        self.handle
            .link()
            .set(LinkMessageBuilder::<LinkUnspec>::default()
                .index(veth_host_id)
                .up()
                .build()
            )
            .execute()
            .await?;

        self.handle
            .link()
            .set(LinkMessageBuilder::<LinkUnspec>::default()
                .controller(bridge_id)
                .build()
            )
            .execute()
            .await?;

        self.handle
            .link()
            .set(LinkMessageBuilder::<LinkUnspec>::default()
                .setns_by_pid(container_pid)
                .build()
            )
            .execute()
            .await?;

        Ok(())
    }

    async fn get_link_index(&self, name: &str) -> Result<u32, Error> {
        let mut links = self.handle
            .link()
            .get()
            .match_name(name)
            .execute();

        if let Some(link) = links.try_next().await? {
            Ok(link.header.index)
        } else {
            Err(Error::NamespaceError(format!("[NETWORK ERROR] Interface not found: {}", name)))
        }
    }
}