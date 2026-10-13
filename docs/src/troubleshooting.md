# Troubleshooting

Common problems you may hit when building or running containers, and what to
do about them.

## "You must run crabtainer as root"

The runtime operations touch namespaces, cgroups, overlayfs, and iptables —
all privileged. Run every command with `sudo`:

```bash
sudo crabtainer run myapp-v1
```

## Alpine-based systems aren't supported

`crabtainer_init` and the runtime rely on glibc-specific behaviors; Alpine
(which ships musl by default) is currently unsupported. Use a Debian/Ubuntu
host, or report the issue upstream.

## Containers can't reach the host or loopback services

The bridge requires `route_localnet` on the host to route traffic to/from
loopback. Enable the drop-in and apply it:

```bash
sudo crabtainer system init
# writes /etc/sysctl.d/99-crabtainer.conf:
#   net.ipv4.conf.all.route_localnet = 1
sysctl -p /etc/sysctl.d/99-crabtainer.conf
```

## No outbound connectivity from containers

Check and enable IPv4 forwarding, and verify the NAT rules were installed:

```bash
sysctl net.ipv4.ip_forward          # should be 1
sudo sysctl -w net.ipv4.ip_forward=1
iptables -t nat -L POSTROUTING      # should show a MASQUERADE rule
```

## Port forwarding doesn't work

* Make sure the host isn't already listening on the host-side port.
* Port ranges must have equal sizes (`-p 1000-2000:1000-2000`); mixing a
  single port with a range is rejected.
* Check the `DNAT` rules in the `CRABTAINER` chain:

```bash
iptables -t nat -L CRABTAINER
```

## Subnet conflict (172.19.0.0/16)

The bridge subnet, gateway, and bridge name (`crabtainer0`) are hardcoded. If
`172.19.0.0/16` collides with another network on the host (LAN, Docker,
VPN…), containers will misbehave. Stopping the conflicting network, or
relocating Crabtainer to a host where the subnet is free, resolves it.

## IPAM got out of sync

If containers start getting duplicate IPs (or you restored from a snapshot),
reset the lease database while all containers are stopped:

```bash
sudo crabtainer ps            # confirm nothing is Active
sudo rm /var/lib/crabtainer/ipam.json
```

The pool is re-seeded the next time a container starts.

## A container shows `Active` after a host reboot

Crabtainer is daemonless: without a running process, nothing reaps it
immediately. The next `crabtainer ps` (or any containers command) runs a
refresh that compares the stored `boot_id` with the current one, marks such
containers `Exited`, and — for detached containers with a suitable restart
policy — re-spawns them (`crabtainer system autostart`).

## `crabtainer build` fails with "Layout already exists"

Tags are unique. Remove the existing layout first — but a layout that is
still referenced by a container cannot be removed:

```bash
sudo crabtainer layout ps
sudo crabtainer layout rm myapp-v1
sudo crabtainer build -t myapp-v1
```

## `crabtainer init` not on `PATH`

The examples in these docs use the built binaries directly. Either build with
`cargo build --release` and reference `./target/release/crabtainer`, or copy
the two binaries (`crabtainer` and `crabtainer_init` — they must live next to
each other) onto your `PATH`.

## Still stuck?

Open an issue at
<https://github.com/LisZLisowni2/crabtainer> and include:

* The exact `crabtainer` command and its output
* `uname -a` and the OS/distro
* Whether `cgroups v2` is active (`mount | grep cgroup2`)