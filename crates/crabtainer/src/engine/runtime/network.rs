//! Container networking: bridge setup, veth wiring, IPAM, and iptables
//! rules.
//!
//! [`NetworkManager`] creates the `crabtainer0` bridge, attaches per-container
//! veth pairs into their network namespaces, and manages iptables NAT/forward
//! rules (including port forwarding). [`Ipam`] leases IP addresses from a
//! subnet, persisting allocations to a JSON database so they survive
//! restarts.

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

/// Manages bridge networking for the container runtime.
///
/// Owns an rtnetlink [`Handle`] plus the identity of the shared bridge
/// (`crabtainer0`) and its subnet. One instance is created per container
/// spawn (or per host-level operation) and drives the netlink `Handle`.
pub struct NetworkManager {
    /// rtnetlink connection handle used for all netlink requests.
    handle: Handle,
    /// Name of the bridge interface (`crabtainer0`).
    bridge_name: String,
    /// Gateway IP of the container subnet on the bridge.
    gateway_ip: Ipv4Addr,
    /// Prefix length of the container subnet (e.g. `16` for `/16`).
    subnet_prefix: u8,
}

/// A single `host:container` port forwarding mapping.
pub struct PortForwarding {
    /// Port on the host side.
    host: String,
    /// Port on the container side.
    container: String,
}

impl PortForwarding {
    /// Creates a port forwarding entry from a host port and a container port.
    pub fn new(host_port: String, container_port: String) -> Self {
        Self {
            host: host_port,
            container: container_port,
        }
    }
}

/// Errors that can occur during IP address management.
#[derive(Error, Debug)]
pub enum IpamError {
    /// The subnet has no free addresses left to allocate.
    #[error("Subnet exhausted: no available IPs left")]
    SubnetExhausted,
    /// The requested address is already leased to another container.
    #[error("IP {0} is already allocated")]
    AlreadyAllocated(Ipv4Addr),
    /// The given container has no lease to release.
    #[error("Container {0} has no allocated IP")]
    ContainerNotFound(String),
    /// Underlying filesystem error while loading or persisting state.
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    /// State database could not be serialized or deserialized.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Serializable snapshot of the IPAM state, stored in `<base>/ipam.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct IpamState {
    /// The subnet addresses are allocated from.
    subnet: Ipv4Net,
    /// Gateway address reserved at the start of the subnet.
    gateway: Ipv4Addr,
    /// Container id → leased IP address.
    allocations: HashMap<String, Ipv4Addr>,
}

/// Allocates and releases container IP addresses from a subnet.
///
/// State is persisted to a JSON database on every mutation, so leases survive
/// process restarts. A temporary file plus rename keeps the write atomic.
pub struct Ipam {
    /// Path of the JSON state database.
    db_path: PathBuf,
    /// In-memory state, shared behind an async mutex.
    state: Arc<Mutex<IpamState>>,
}

impl Ipam {
    /// Loads an existing IPAM database or creates a new one for `subnet_cidr`.
    ///
    /// For a new database the first address of the subnet is reserved as the
    /// gateway and the initial state is persisted immediately.
    ///
    /// # Arguments
    ///
    /// * `subnet_cidr` - Subnet in CIDR form, e.g. `172.19.0.0/16`.
    /// * `db_path` - Where the state database lives (or should be created).
    ///
    /// # Errors
    ///
    /// Returns [`IpamError`] when the CIDR is invalid or the database cannot
    /// be read or written.
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

    /// Returns the gateway IP of the subnet.
    pub async fn gateway(&self) -> Ipv4Addr {
        let state = self.state.lock().await;

        state.gateway
    }

    /// Returns the prefix length of the subnet (e.g. `16` for `/16`).
    pub async fn subnet(&self) -> u8 {
        let state = self.state.lock().await;

        state.subnet.prefix_len()
    }

    /// Allocates an IP address for `container_id`, handing back an existing
    /// lease if the container already has one.
    ///
    /// The gateway address is never handed out. Returns
    /// [`IpamError::SubnetExhausted`] when no address is free. The updated
    /// state is persisted before returning.
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

    /// Releases the IP lease of `container_id`, returning the freed address.
    ///
    /// Returns [`IpamError::ContainerNotFound`] when the container has no
    /// lease. The updated state is persisted before returning.
    pub async fn release(&self, container_id: &String) -> Result<Ipv4Addr, IpamError> {
        let mut state = self.state.lock().await;

        let released_ip = state
            .allocations
            .remove(container_id)
            .ok_or_else(|| IpamError::ContainerNotFound(container_id.clone()))?;

        self.persist_sync(&state)?;
        Ok(released_ip)
    }

    /// Persists `state` to the database atomically (temp file + rename).
    ///
    /// Creates the parent directory if needed and fsyncs the file before
    /// renaming it into place.
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
    /// Creates a [`NetworkManager`] on a fresh rtnetlink connection.
    ///
    /// # Arguments
    ///
    /// * `bridge_name` - Name of the bridge interface (`crabtainer0`).
    /// * `gateway_ip` - Gateway IP assigned to the bridge.
    /// * `subnet_prefix` - Prefix length of the container subnet.
    ///
    /// # Errors
    ///
    /// Fails when a netlink connection cannot be established.
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

    /// Creates the host-side container network: the bridge interface, its
    /// gateway address, bringing it up, and the iptables NAT/forward rules.
    ///
    /// Skips everything (with a warning) when the bridge already exists.
    ///
    /// # Errors
    ///
    /// Fails if any netlink operation or iptables call fails.
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

    /// Adds the iptables rules that give containers outbound connectivity.
    ///
    /// Creates a `CRABTAINER` chain in the `filter` table and appends the
    /// rules needed for outbound MASQUERADE, container-subnet forwarding,
    /// connection tracking of return traffic, and a hook from the `FORWARD`
    /// chain into the `CRABTAINER` chain. Existing rules are left untouched.
    ///
    /// # Arguments
    ///
    /// * `bridge_name` - Name of the bridge (`crabtainer0`).
    /// * `subnet_mask` - Container subnet in CIDR form, e.g. `172.19.0.0/16`.
    ///
    /// # Errors
    ///
    /// Fails if iptables cannot be invoked or a chain/rule cannot be managed.
    pub async fn add_iptables_rules(
        &self,
        bridge_name: &str,
        subnet_mask: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ipt = iptables::new(false)?;

        if !ipt.chain_exists("filter", "CRABTAINER")? {
            ipt.new_chain("filter", "CRABTAINER")?;
            println!("[IPTABLES] Created new chain: CRABTAINER");
        }

        let rule = format!("-s {} ! -o {} -j MASQUERADE", subnet_mask, bridge_name);

        if !ipt.exists("nat", "POSTROUTING", rule.as_str())? {
            ipt.append("nat", "POSTROUTING", rule.as_str())?;
            println!("[IPTABLES] Added NAT rule: {}", subnet_mask);
        }

        let forward_rule = format!("-s {} ! -o {} -j ACCEPT", subnet_mask, bridge_name);
        if !ipt.exists("filter", "CRABTAINER", &forward_rule)? {
            ipt.append("filter", "CRABTAINER", &forward_rule)?;
            println!(
                "[IPTABLES] Allowed forwarding from {} to {}",
                subnet_mask, bridge_name
            );
        }

        let return_rule = format!(
            "-o {} -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT",
            bridge_name
        );
        if !ipt.exists("filter", "CRABTAINER", &return_rule)? {
            ipt.append("filter", "CRABTAINER", &return_rule)?;
            println!("[IPTABLES] Added filter rule for {}", bridge_name);
        }

        if !ipt.exists("filter", "FORWARD", "-j CRABTAINER")? {
            ipt.append("filter", "FORWARD", "-j CRABTAINER")?;
            println!("[IPTABLES] Appended CRABTAINER chain to FORWARD");
        }

        let second_forward_rule = "-m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT";

        if !ipt.exists("filter", "FORWARD", second_forward_rule)? {
            ipt.append("filter", "FORWARD", second_forward_rule)?;
            println!("[IPTABLES] Added forward rule for port forwarding")
        }

        Ok(())
    }

    /// Removes the iptables rules created by
    /// [`add_iptables_rules`](Self::add_iptables_rules).
    ///
    /// Note: this implementation's rule strings do not always match the ones
    /// added, so it is mostly best-effort cleanup.
    ///
    /// # Arguments
    ///
    /// * `bridge_name` - Name of the bridge (`crabtainer0`).
    /// * `subnet_mask` - Container subnet in CIDR form.
    ///
    /// # Errors
    ///
    /// Fails if iptables cannot be invoked.
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

        let second_forward_rule = "-m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT";

        if ipt.exists("filter", "FORWARD", second_forward_rule)? {
            ipt.append("filter", "FORWARD", second_forward_rule)?;
            println!("[IPTABLES] Added forward rule for port forwarding")
        }

        Ok(())
    }

    /// Attaches a container to the bridge using a caller-provided netlink
    /// [`Handle`].
    ///
    /// Builds a temporary [`NetworkManager`] around the supplied handle (e.g.
    /// one already entered into the container's network namespace) and
    /// delegates to [`attach_container`](Self::attach_container). Failures
    /// are logged, not propagated.
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

    /// Parses raw port strings into [`PortForwarding`] entries.
    ///
    /// Supported formats (see the comment inside the function):
    ///
    /// - single pair: `3432:5835`
    /// - equal-sized ranges: `2000-3000:2000-3000` (expanded pair by pair)
    ///
    /// Ranges of different sizes or mixed single/range forms are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error string for malformed or mismatched port specs.
    pub async fn handle_ports(&self, ports: Vec<String>) -> Result<Vec<PortForwarding>, String> {
        /*
         * Possible values:
         * - 3432:5835
         * - 2000-3000:2000-3000
         * */
        /*
         *   TODO: Handle offsets in ranges (like 2000-3000:2100-3100)
         * */
        let mut parsed_ports: Vec<PortForwarding> = Vec::new();

        for port_str in ports {
            let parts: Vec<&str> = port_str.split(':').collect();

            if parts.len() == 2 {
                let first_part = parts[0].to_string();
                let second_part = parts[1].to_string();
                if first_part.contains('-') && second_part.contains('-') {
                    let separated_first_part: Vec<&str> = first_part.split('-').collect();
                    let separated_second_part: Vec<&str> = second_part.split('-').collect();
                    let lower_bound_first = separated_first_part[0]
                        .parse::<u32>()
                        .expect("[ERROR] Failed to parse to u32 lower_bound_first");
                    let upper_bound_first = separated_first_part[1]
                        .parse::<u32>()
                        .expect("[ERROR] Failed to parse to u32 upper_bound_first");
                    let lower_bound_second = separated_second_part[0]
                        .parse::<u32>()
                        .expect("[ERROR] Failed to parse to u32 lower_bound_second");
                    let upper_bound_second = separated_second_part[1]
                        .parse::<u32>()
                        .expect("[ERROR] Failed to parse to u32 upper_bound_second");

                    if upper_bound_first - lower_bound_first
                        != upper_bound_second - lower_bound_second
                    {
                        return Err("[ERROR] Ranges of ports have different sizes".to_string());
                    }

                    for i in lower_bound_first..=upper_bound_first {
                        let port = PortForwarding::new(i.to_string(), i.to_string());
                        parsed_ports.push(port);
                    }
                } else if !first_part.contains('-') && !second_part.contains('-') {
                    let port = PortForwarding::new(first_part, second_part);
                    parsed_ports.push(port);
                } else {
                    return Err("[ERROR] You can set range only or single port only".to_string());
                }
            } else {
                return Err(
                    "[ERROR] Incorrect port assignment (if you're looking for assigning IPs, this feature isn't supported)".to_string()
                );
            }
        }

        Ok(parsed_ports)
    }

    /// Adds the iptables DNAT rules that forward host ports to `container_ip`.
    ///
    /// For every port pair this installs a `PREROUTING` DNAT rule, an
    /// `OUTPUT` rule for localhost traffic, and a `POSTROUTING` MASQUERADE
    /// rule for the container. Existing rules are left untouched.
    ///
    /// # Errors
    ///
    /// Fails if iptables cannot be invoked.
    pub async fn add_port_forwarding(
        &self,
        ports: Vec<PortForwarding>,
        container_ip: Ipv4Addr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ipt = iptables::new(false)?;

        for port in ports {
            let rule = format!(
                "-p tcp --dport {} -j DNAT --to-destination {}:{}",
                port.host, container_ip, port.container
            );

            if !ipt.exists("nat", "PREROUTING", rule.as_str())? {
                ipt.append("nat", "PREROUTING", rule.as_str())?;
                println!(
                    "[IPTABLES] Added port route for ports {}:{}",
                    port.host, port.container
                );
            }

            let output_rule = format!(
                "-p tcp -d 127.0.0.1/32 --dport {} -j DNAT --to-destination {}:{}",
                port.host, container_ip, port.container
            );

            if !ipt.exists("nat", "OUTPUT", output_rule.as_str())? {
                ipt.append("nat", "OUTPUT", output_rule.as_str())?;
                println!(
                    "[IPTABLES] Added output rule for ports {}:{}",
                    port.host, port.container
                )
            }

            let post_rule = format!(
                "-p tcp -d {} --dport {} -j MASQUERADE",
                container_ip, port.container
            );

            if !ipt.exists("nat", "POSTROUTING", post_rule.as_str())? {
                ipt.append("nat", "POSTROUTING", post_rule.as_str())?;
                println!(
                    "[IPTABLES] Added postrouting rule for container IP '{}' and port '{}'",
                    container_ip, port.container
                )
            }
        }
        Ok(())
    }

    /// Removes the iptables DNAT rules set up by
    /// [`add_port_forwarding`](Self::add_port_forwarding) for `container_ip`.
    ///
    /// # Errors
    ///
    /// Fails if iptables cannot be invoked.
    pub async fn remove_port_forwarding(
        &self,
        ports: Vec<PortForwarding>,
        container_ip: Ipv4Addr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ipt = iptables::new(false)?;

        for port in ports {
            let rule = format!(
                "-p tcp --dport {} -j DNAT --to-destination {}:{}",
                port.host, container_ip, port.container
            );

            if ipt.exists("nat", "PREROUTING", rule.as_str())? {
                ipt.delete("nat", "PREROUTING", rule.as_str())?;
                println!(
                    "[IPTABLES] Deleted port route for ports {}:{}",
                    port.host, port.container
                );
            }

            let forward_rule = format!(
                "-p tcp -d {} --dport {} -m conntrack --ctstate NEW,ESTABLISHED,RELATED -j ACCEPT",
                container_ip, port.container
            );

            if ipt.exists("filter", "FORWARD", forward_rule.as_str())? {
                ipt.delete("filter", "FORWARD", forward_rule.as_str())?;
                println!(
                    "[IPTABLES] Deleted forward rule for container's ip {} and port {}",
                    container_ip, port.container
                )
            }

            let output_rule = format!(
                "-p tcp -o lo --dport {} -j DNAT --to-destination {}:{}",
                port.host, container_ip, port.container
            );

            if ipt.exists("nat", "OUTPUT", output_rule.as_str())? {
                ipt.delete("nat", "OUTPUT", output_rule.as_str())?;
                println!(
                    "[IPTABLES] Deleted output rule for ports {}:{}",
                    port.host, port.container
                )
            }
        }
        Ok(())
    }

    /// Attaches a running container to the bridge network.
    ///
    /// Creates a veth pair (`veth_<short-id>` on the host, `vethc_<short-id>`
    /// in the container), slaves the host side to the bridge, moves the
    /// container side into the container's network namespace, then — inside
    /// that namespace — brings the interface up, assigns `ip`, adds a default
    /// route via the gateway, and brings `lo` up.
    ///
    /// Returns early when the container has no network namespace (e.g. it
    /// already exited).
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container id, used for the veth names.
    /// * `container_pid` - Host PID of the container's root process.
    /// * `ip` - IP address to assign inside the container.
    ///
    /// # Errors
    ///
    /// Fails if any netlink operation fails, the container's netns cannot be
    /// opened, or the in-namespace setup thread errors.
    pub async fn attach_container(
        &self,
        container_id: &str,
        container_pid: i32,
        ip: Ipv4Addr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if fs::metadata(format!("/proc/{}/ns/net", container_pid)).is_err() {
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

    /// Looks up the interface index of `name` via rtnetlink.
    ///
    /// # Errors
    ///
    /// Returns an rtnetlink error if the interface does not exist.
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

    /// Creates an [`Ipam`] for `subnet` backed by `db_path` for a test.
    fn new_ipam(subnet: &str, db_path: PathBuf) -> Ipam {
        Ipam::new(subnet, db_path).unwrap()
    }

    #[tokio::test]
    /// A fresh IPAM derives the gateway and prefix from the CIDR.
    async fn new_derives_gateway_and_subnet_from_cidr() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        assert_eq!(ipam.gateway().await, Ipv4Addr::new(172, 19, 0, 1));
        assert_eq!(ipam.subnet().await, 16);
    }

    #[tokio::test]
    /// Invalid CIDR strings are rejected at construction.
    async fn new_rejects_invalid_cidr() {
        let dir = tempdir().unwrap();
        let result = Ipam::new("not-a-cidr", dir.path().join("ipam.json"));
        assert!(matches!(result, Err(IpamError::Io(_))));
    }

    #[tokio::test]
    /// Allocation starts after the gateway and increments per container.
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
    /// Re-allocating for the same container returns its existing lease.
    async fn allocate_is_idempotent_for_same_container() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        let first = ipam.allocate(&"c1".to_string()).await.unwrap();
        let second = ipam.allocate(&"c1".to_string()).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(first, Ipv4Addr::new(172, 19, 0, 2));
    }

    #[tokio::test]
    /// A tiny subnet runs out of allocatable addresses.
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
    /// Releasing an IP makes it available for the next allocation.
    async fn release_returns_ip_and_frees_it_for_reuse() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        let ip = ipam.allocate(&"c1".to_string()).await.unwrap();
        assert_eq!(ipam.release(&"c1".to_string()).await.unwrap(), ip);

        let next = ipam.allocate(&"c2".to_string()).await.unwrap();
        assert_eq!(next, ip);
    }

    #[tokio::test]
    /// Releasing a lease for an unknown container errors.
    async fn release_unknown_container_errors() {
        let dir = tempdir().unwrap();
        let ipam = new_ipam("172.19.0.0/16", dir.path().join("ipam.json"));
        let err = ipam.release(&"ghost".to_string()).await.unwrap_err();
        assert!(matches!(err, IpamError::ContainerNotFound(_)));
    }

    #[tokio::test]
    /// IPAM state survives a reload from the database.
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
