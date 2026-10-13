# Networking

Crabtainer provisions one virtual network per host and gives every container
its own IP through an internal IPAM (IP address management) store.

## Topology

```text
        ┌─────────────────────────── host ────────────────────────────┐
        │                                                             │
        │   crabtainer0 (Linux bridge) 172.19.0.1/16                  │
        │        │                │                │                  │
        │   vethc_<id1>      vethc_<id2>      vethc_<id3>            │
        │        │                │                │                  │
        │   container A       container B      container C            │
        │   172.19.0.2        172.19.0.3       172.19.0.4             │
        │                                                             │
        └─────────────────────────────────────────────────────────────┘
```

* **Bridge:** `crabtainer0` — created automatically on the first `run` if it
  does not exist yet.
* **Subnet:** `172.19.0.0/16`, gateway `172.19.0.1` (hardcoded). Containers
  are assigned addresses starting at `172.19.0.2`.
* **veth pairs:** each container gets a pair — one end (`vethc_<id>`) plugged
  into the bridge, the other placed inside the container's network namespace.
* **IPAM:** leases are tracked in `<base>/ipam.json` so addresses aren't
  double-assigned and free ones are reused.

## NAT and outbound traffic

Containers reach the outside world through iptables NAT rules installed when
the network is set up:

* `POSTROUTING` `MASQUERADE` on the bridge-facing interface for outbound
  traffic.
* Container traffic is forwarded over the bridge (IPv4 forwarding assumed on
  the host).

## Port forwarding

Expose container ports on the host with `-p`:

```bash
# Single port: host 8000 → container 80
sudo crabtainer run -p 8000:80 myapp-v1

# Port ranges (equal sizes only)
sudo crabtainer run -p 10000-10100:10000-10100 myapp-v1

# Multiple forwards
sudo crabtainer run -p 8080:80 -p 8443:443 myapp-v1
```

Rules are installed using iptables `DNAT` (in a dedicated `CRABTAINER` chain
on `PREROUTING`/`OUTPUT`), and the same port-mapping set is restored whenever
the container starts again.

> [!NOTE]
> Port ranges must have **equal sizes** on both sides; mixing single ports and
> ranges in one `-p` value is rejected.

## Consequences of the hardcoded subnet

The subnet, gateway, and bridge name are constants in the binary today, so:

* The `172.19.0.0/16` range must not collide with your LAN or any Docker/podman
  network on the same host.
* Containers restarting (`start`, restart policies) are re-attached to the
  same bridge and re-assigned from the same IPAM pool.

## Troubleshooting

* **Containers can't reach the host/loopback services:** make sure
  `net.ipv4.conf.all.route_localnet` is enabled — run
  `sudo crabtainer system init` (it writes
  `/etc/sysctl.d/99-crabtainer.conf` with
  `net.ipv4.conf.all.route_localnet = 1`).
* **No outbound connectivity:** verify IPv4 forwarding
  (`sysctl net.ipv4.ip_forward`) is enabled and the `MASQUERADE` rule exists
  (`iptables -t nat -L POSTROUTING`).
* **IP conflicts:** delete `<base>/ipam.json` while all containers are
  stopped, and let the pool re-seed itself.