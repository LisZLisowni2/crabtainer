# Storage and Volumes

All persistent state lives under Crabtainer's **base directory**, which
defaults to `/var/lib/crabtainer` and can be overridden with the
`CRABTAINER_HOME` environment variable (useful for testing, e.g. different
homes for `image`/`layout`/`container` stores).

The layout was also the original inspiration for the name `crabtainer`:
everything runs from a single directory tree.

## Directory layout

```text
/var/lib/crabtainer/
├── images/          # Downloaded registry images
│   └── <alias>/
│       ├── config.json       # OCI image config
│       ├── layer_0.tar.gz    # Layer blobs, applied in order
│       └── layer_N.tar.gz
├── layouts/         # Built layouts (ready-to-run rootfs + spec)
│   └── <tag>/
│       ├── config.json       # OCI runtime spec (from `FROM`/image config)
│       └── rootfs/           # Merged filesystem, used as overlay lowerdir
├── containers/      # Per-container runtime state
│   └── <id>/
│       ├── config.json       # RuntimeConfig (pid, status, limits, mounts…)
│       ├── rootfs/           # Overlay mountpoint (merged view)
│       ├── upper/            # Overlayfs upper layer (writes)
│       ├── work/             # Overlayfs work dir
│       ├── pid               # (detached) PID of the root process
│       └── container.log     # (detached) stdout/stderr
├── volumes/         # Named volumes
│   └── <name>/
└── ipam.json        # IP address leases for the bridge subnet
```

The directories are created on demand by `CrabtainerPaths::init_system_dirs()`
(the first `image pull` / `build` / `run` call will create them).

## Images vs. layouts

* **Images** are what `image pull` (or the `DOWNLOAD` instruction) fetches
  from a registry: an OCI config plus compressed layer blobs stored in
  `images/<alias>/`.
* **Layouts** are built by `crabtainer build`: the layers are unpacked onto a
  shared rootfs in `layouts/<tag>/`, accompanied by an OCI runtime
  specification derived from the base image's config. Layouts are immutable
  build artifacts — running a container never writes to the layout.
* **Containers** are created by `crabtainer run` on top of a layout. Each
  container gets its own `upper/`, `work/`, and `rootfs/` directories; the
  layout's rootfs is mounted as the overlay's read-only lowerdir so the
  layout itself stays pristine and can be shared by many containers.

## Container state (`containers/<id>/config.json`)

When a container is created, its configuration (the `RuntimeConfig`) is
serialized to `config.json` and re-read on every subsequent command, which is
what makes the whole model *daemonless* — there is no central manager to
consult, only the files on disk. It records:

* Container id, name (if any), and the layout tag it was created from
* `pid` and `boot_id` (used to detect stale processes after a host reboot)
* `status`: `Active`, `Stopped`, `Exited`, or `Error`
* `ip_address` (assigned from the IPAM pool)
* Restart policy, detached flag, and `--rm` flag
* The command line, environment, CPU/memory limits, ports, and volumes

## Volumes

Two kinds of mounts can be attached with `-v`/`--volume`:

```text
-v <name>:/container/path        # named volume
-v /host/path:/container/path    # bind mount (source contains '/' or '~')
```

### Named volumes

* Stored under `volumes/<name>/`.
* Created on first use: if the named volume does not exist yet, the container
  directory at the destination path is **seeded** into the volume (i.e. the
  image's existing files are copied in once), then the volume is mounted over
  that path.
* Persist independently of the container; removing the container does not
  remove the volume.
* Manage them with `crabtainer volume create|ls|remove`.

### Bind mounts

* Any source containing `/` (or `~`) is treated as a host path and bound
  directly (after canonicalization).
* Useful for sharing source code, sockets, or other host state into the
  container.

## Garbage collection

* `crabtainer rm <name>` / `rm .` removes **stopped/exited** containers
  (their directory trees, including overlay upper/work dirs).
* `crabtainer layout rm <tag>` / `rm .` removes built layouts, refusing to
  delete layouts still referenced by active containers.
* `crabtainer image rm <name>` / `rm .` removes downloaded images.
* `system prune` and `volume prune` are currently **no-op stubs** — not
  implemented yet.