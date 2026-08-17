use futures::stream::TryStreamExt;
use ipnet::Ipv4Net;
use rtnetlink::{
    Handle, LinkBridge, LinkMessageBuilder, LinkUnspec, LinkVeth, RouteMessageBuilder,
    new_connection,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::Ipv4Addr;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use crate::engine::support::paths::RustockerPaths;

pub struct NetworkManager {
    handle: Handle,
    bridge_name: String,
    gateway_ip: Ipv4Addr,
    subnet_prefix: u8,
}

#[derive(Error, Debug)]
pub enum IpamError {
    #[error("Subnet exhausted: no available IPs left")]
    SubnetExhausted,
    #[error("IP {0} is already allocated")]
    AlreadyAllocated(Ipv4Addr),
    #[error("Container {0} has no allocated IP")]
    ContainerNotFound(String),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpamState {
    subnet: Ipv4Net,
    gateway: Ipv4Addr,
    allocations: HashMap<String, Ipv4Addr>,
}

pub struct Ipam {
    db_path: PathBuf,
    state: Arc<Mutex<IpamState>>,
}

impl Ipam {
    pub fn new<P: AsRef<Path>>(subnet_cidr: &str, db_path: P) -> Result<Self, IpamError> {
        let path = db_path.as_ref().to_path_buf();

        let state = if path.exists() {
            let mut file = File::open(&path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            serde_json::from_str(&contents)?
        } else {
            let subnet: Ipv4Net = subnet_cidr.parse().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid CIDR block")
            })?;

            let mut hosts = subnet.hosts();
            let gateway = hosts.next().ok_or(IpamError::SubnetExhausted)?;

            let new_state = IpamState {
                subnet,
                gateway,
                allocations: HashMap::new(),
            };

            let ipam = Self {
                db_path: db_path.as_ref().to_path_buf(),
                state: Arc::new(Mutex::new(new_state)),
            };

            let guard = futures::executor::block_on(ipam.state.lock());
            ipam.persist_sync(&guard)?;
            drop(guard);

            return Ok(ipam);
        };

        Ok(Self {
            db_path: path,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub async fn gateway(&self) -> Ipv4Addr {
        let state = self.state.lock().await;

        state.gateway
    }

    pub async fn subnet(&self) -> u8 {
        let state = self.state.lock().await;

        state.subnet.prefix_len()
    }

    pub async fn allocate(&self, container_id: &String) -> Result<Ipv4Addr, IpamError> {
        let mut state = self.state.lock().await;

        if let Some(&ip) = state.allocations.get(container_id) {
            return Ok(ip);
        }

        let allocated_set: std::collections::HashSet<Ipv4Addr> =
            state.allocations.values().copied().collect();

        let _gateway = state.gateway;
        let next_ip = state
            .subnet
            .hosts()
            .skip(1)
            .find(|ip| !allocated_set.contains(ip))
            .ok_or(IpamError::SubnetExhausted)?;

        state.allocations.insert(container_id.clone(), next_ip);
        self.persist_sync(&state)?;

        Ok(next_ip)
    }

    pub async fn release(&self, container_id: &String) -> Result<Ipv4Addr, IpamError> {
        let mut state = self.state.lock().await;

        let released_ip = state
            .allocations
            .remove(container_id)
            .ok_or_else(|| IpamError::ContainerNotFound(container_id.clone()))?;

        self.persist_sync(&state)?;
        Ok(released_ip)
    }

    fn persist_sync(&self, state: &IpamState) -> Result<(), IpamError> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = self.db_path.with_extension("tmp");
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;

            let serialized = serde_json::to_string_pretty(state)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
        }

        std::fs::rename(&tmp_path, &self.db_path)?;
        Ok(())
    }
}

impl NetworkManager {
    pub async fn new(
        bridge_name: String,
        gateway_ip: Ipv4Addr,
        subnet_prefix: u8,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        Ok(Self {
            handle,
            bridge_name,
            gateway_ip,
            subnet_prefix,
        })
    }

    pub async fn init_global_network(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.get_link_index(self.bridge_name.as_str()).await.is_ok() {
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
            .add(
                bridge_idx,
                std::net::IpAddr::V4(self.gateway_ip),
                self.subnet_prefix,
            )
            .execute()
            .await?;

        println!("[NETWORK] Set IP for bridge: {}", self.bridge_name);

        self.handle
            .link()
            .set(
                LinkMessageBuilder::<LinkUnspec>::default()
                    .index(bridge_idx)
                    .up()
                    .build(),
            )
            .execute()
            .await?;
        println!("[NETWORK] Set UP bridge: {}", self.bridge_name);

        println!("[NETWORK] Setting iptables rules");
        self.add_iptables_rules(
            self.bridge_name.as_str(),
            format!("172.19.0.0/{}", self.subnet_prefix).as_str(),
        )
        .await?;

        Ok(())
    }

    pub async fn add_iptables_rules(
        &self,
        bridge_name: &str,
        subnet_mask: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ipt = iptables::new(false)?;

        if !ipt.chain_exists("filter", "RUSTOCKER")? {
            ipt.new_chain("filter", "RUSTOCKER")?;
            println!("[IPTABLES] Created new chain: RUSTOCKER");
        }

        let rule = format!("-s {} ! -o {} -j MASQUERADE", subnet_mask, bridge_name);

        if !ipt.exists("nat", "POSTROUTING", rule.as_str())? {
            ipt.append("nat", "POSTROUTING", rule.as_str())?;
            println!("[IPTABLES] Added NAT rule: {}", subnet_mask);
        }

        let forward_rule = format!("-s {} ! -o {} -j ACCEPT", subnet_mask, bridge_name);
        if !ipt.exists("filter", "RUSTOCKER", &forward_rule)? {
            ipt.append("filter", "RUSTOCKER", &forward_rule)?;
            println!(
                "[IPTABLES] Allowed forwarding from {} to {}",
                subnet_mask, bridge_name
            );
        }

        let return_rule = format!(
            "-o {} -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT",
            bridge_name
        );
        if !ipt.exists("filter", "RUSTOCKER", &return_rule)? {
            ipt.append("filter", "RUSTOCKER", &return_rule)?;
            println!("[IPTABLES] Added filter rule for {}", bridge_name);
        }

        if !ipt.exists("filter", "FORWARD", "-j RUSTOCKER")? {
            ipt.append("filter", "FORWARD", "-j RUSTOCKER")?;
            println!("[IPTABLES] Appended RUSTOCKER chain to FORWARD");
        }

        Ok(())
    }

    pub async fn remove_iptables_rules(
        &self,
        bridge_name: &str,
        subnet_mask: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ipt = iptables::new(false)?;
        let rule = format!("-s {} -o {} -j MASQUERADE", subnet_mask, bridge_name);

        if ipt.exists("nat", "POSTROUTING", &rule)? {
            ipt.delete("nat", "POSTROUTING", &rule)?;
            println!("[IPTABLES] Removed MASQUERADE rule.");
        }

        let forward_rule = format!("-s {} ! -o {} -j FORWARD", subnet_mask, bridge_name);
        if ipt.exists("filter", "FORWARD", &forward_rule)? {
            ipt.delete("filter", "FORWARD", &forward_rule)?;
            println!("[IPTABLES] Removed forwarding for {}", bridge_name);
        }

        let return_rule = format!(
            "-o {} -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT",
            bridge_name
        );
        if ipt.exists("filter", "FORWARD", &return_rule)? {
            ipt.delete("filter", "FORWARD", &return_rule)?;
            println!("[IPTABLES] Removed filter rule for {}", bridge_name);
        }

        Ok(())
    }

    pub async fn attach_container_with_custom_handle(
        &self,
        container_id: &str,
        container_pid: i32,
        ip: Ipv4Addr,
        handle: Handle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_network_manager = Self {
            gateway_ip: self.gateway_ip,
            bridge_name: self.bridge_name.clone(),
            subnet_prefix: self.subnet_prefix,
            handle,
        };

        if let Err(e) = temp_network_manager
            .attach_container(container_id, container_pid, ip)
            .await
        {
            eprintln!("[NETWORK] Failed to attach container to IP address: {}", e);
        };

        Ok(())
    }

    pub async fn attach_container(
        &self,
        container_id: &str,
        container_pid: i32,
        ip: Ipv4Addr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Err(e) = fs::read_dir(format!("/proc/{}/ns/net", container_pid)) {
            return Ok(());
        }
        
        let short_id = &container_id[..6.min(container_id.len())];
        let veth_host = format!("veth_{}", short_id);
        let veth_cont = format!("vethc_{}", short_id);

        println!(
            "[NETWORK] Creating new pair link for container {}: {} <-> {}",
            container_id, veth_host, veth_cont
        );

        self.handle
            .link()
            .add(LinkVeth::new(veth_host.as_str(), veth_cont.as_str()).build())
            .execute()
            .await?;
        println!(
            "[NETWORK] Created pair link: {} <-> {}",
            veth_host, veth_cont
        );

        let bridge_id = self.get_link_index(self.bridge_name.as_str()).await?;
        let veth_host_id = self.get_link_index(veth_host.as_str()).await?;
        let veth_cont_id = self.get_link_index(veth_cont.as_str()).await?;

        self.handle
            .link()
            .set(
                LinkMessageBuilder::<LinkUnspec>::default()
                    .index(veth_host_id)
                    .controller(bridge_id)
                    .up()
                    .build(),
            )
            .execute()
            .await?;
        println!("[NETWORK] Set up and master for veth {}", veth_host);

        let netns_path = format!("/proc/{}/ns/net", container_pid);
        let netns_fd = File::open(&netns_path)
            .map_err(|e| format!("[ERROR] Failed to open netns path '{}': {}", netns_path, e))?;

        self.handle
            .link()
            .set(
                LinkMessageBuilder::<LinkUnspec>::default()
                    .index(veth_cont_id)
                    .setns_by_fd(netns_fd.as_raw_fd())
                    .build(),
            )
            .execute()
            .await
            .map_err(|e| format!("[ERROR] setns: {}", e))?;
        println!("[NETWORK] Setns to container: {}", container_id);

        let gateway_ip = self.gateway_ip;
        let subnet_prefix = self.subnet_prefix;

        tokio::task::spawn_blocking(
            move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                let thread_handle = std::thread::spawn(
                    move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                        use nix::sched::{CloneFlags, setns};

                        setns(netns_fd, CloneFlags::CLONE_NEWNET)?;

                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()?;

                        rt.block_on(async {
                            let (conn, handle, _) = rtnetlink::new_connection().unwrap();
                            let conn_handle = tokio::spawn(conn);

                            let _setup_result: Result<
                                (),
                                Box<dyn std::error::Error + Send + Sync>,
                            > = async {
                                let mut links =
                                    handle.link().get().match_name(veth_cont.clone()).execute();
                                let container_veth = links
                                    .try_next()
                                    .await?
                                    .ok_or("[NETWORK] Veth not found in container")?;
                                let c_veth_id = container_veth.header.index;

                                handle
                                    .link()
                                    .set(
                                        LinkMessageBuilder::<LinkUnspec>::default()
                                            .index(c_veth_id)
                                            .up()
                                            .build(),
                                    )
                                    .execute()
                                    .await?;

                                handle
                                    .address()
                                    .add(c_veth_id, std::net::IpAddr::V4(ip), subnet_prefix)
                                    .execute()
                                    .await?;

                                handle
                                    .route()
                                    .add(
                                        RouteMessageBuilder::<Ipv4Addr>::default()
                                            .destination_prefix(Ipv4Addr::UNSPECIFIED, 0)
                                            .gateway(gateway_ip)
                                            .output_interface(c_veth_id)
                                            .build(),
                                    )
                                    .execute()
                                    .await?;

                                let mut lo_links =
                                    handle.link().get().match_name("lo".to_string()).execute();
                                if let Some(lo) = lo_links.try_next().await? {
                                    handle
                                        .link()
                                        .set(
                                            LinkMessageBuilder::<LinkUnspec>::default()
                                                .index(lo.header.index)
                                                .up()
                                                .build(),
                                        )
                                        .execute()
                                        .await?;
                                }

                                Ok(())
                            }
                            .await;

                            conn_handle.abort();

                            Ok(())
                        })
                    },
                );

                let _ = thread_handle
                    .join()
                    .map_err(|e| format!("[NETWORK] Failed to join thread: {:?}", e))?;

                Ok(())
            },
        );

        Ok(())
    }

    async fn get_link_index(&self, name: &str) -> Result<u32, rtnetlink::Error> {
        let mut links = self.handle.link().get().match_name(name).execute();

        if let Some(link) = links.try_next().await? {
            Ok(link.header.index)
        } else {
            Err(rtnetlink::Error::NamespaceError(format!(
                "[NETWORK ERROR] Interface not found: {}",
                name
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn new_ipam(subnet: &str, db_path: PathBuf) -> Ipam {
        Ipam::new(subnet, db_path).unwrap()
    }

    #[tokio::test]
    async fn new_derives_gateway_and_subnet_from_cidr() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        assert_eq!(ipam.gateway().await, Ipv4Addr::new(172, 19, 0, 1));
        assert_eq!(ipam.subnet().await, 16);
    }

    #[tokio::test]
    async fn new_rejects_invalid_cidr() {
        let dir = tempdir().unwrap();
        let result = Ipam::new("not-a-cidr", dir.path().join("ipam.json"));
        assert!(matches!(result, Err(IpamError::Io(_))));
    }

    #[tokio::test]
    async fn allocate_starts_after_gateway_and_increments() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        assert_eq!(
            ipam.allocate(&"c1".to_string()).await.unwrap(),
            Ipv4Addr::new(172, 19, 0, 2)
        );
        assert_eq!(
            ipam.allocate(&"c2".to_string()).await.unwrap(),
            Ipv4Addr::new(172, 19, 0, 3)
        );
    }

    #[tokio::test]
    async fn allocate_is_idempotent_for_same_container() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        let first = ipam.allocate(&"c1".to_string()).await.unwrap();
        let second = ipam.allocate(&"c1".to_string()).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(first, Ipv4Addr::new(172, 19, 0, 2));
    }

    #[tokio::test]
    async fn allocate_exhausts_small_subnet() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/30", dir.path().join("ipam.json"));
        assert_eq!(
            ipam.allocate(&"c1".to_string()).await.unwrap(),
            Ipv4Addr::new(172, 19, 0, 2)
        );
        let err = ipam.allocate(&"c2".to_string()).await.unwrap_err();
        assert!(matches!(err, IpamError::SubnetExhausted));
    }

    #[tokio::test]
    async fn release_returns_ip_and_frees_it_for_reuse() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        let ip = ipam.allocate(&"c1".to_string()).await.unwrap();
        assert_eq!(ipam.release(&"c1".to_string()).await.unwrap(), ip);

        let next = ipam.allocate(&"c2".to_string()).await.unwrap();
        assert_eq!(next, ip);
    }

    #[tokio::test]
    async fn release_unknown_container_errors() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        let err = ipam.release(&"ghost".to_string()).await.unwrap_err();
        assert!(matches!(err, IpamError::ContainerNotFound(_)));
    }

    #[tokio::test]
    async fn state_persists_and_restores_on_reload() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("ipam.json");
        {
            let ipam = new_ipam("172.19.0.0/16", db.clone());
            ipam.allocate(&"c1".to_string()).await.unwrap();
        }

        let ipam = new_ipam("172.19.0.0/16", db);
        assert_eq!(ipam.gateway().await, Ipv4Addr::new(172, 19, 0, 1));
        assert_eq!(ipam.subnet().await, 16);
        assert_eq!(
            ipam.allocate(&"c1".to_string()).await.unwrap(),
            Ipv4Addr::new(172, 19, 0, 2)
        );
        assert_eq!(
            ipam.allocate(&"c2".to_string()).await.unwrap(),
            Ipv4Addr::new(172, 19, 0, 3)
        );
    }
}
