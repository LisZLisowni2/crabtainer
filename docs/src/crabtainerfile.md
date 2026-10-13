# Crabtainerfile Syntax Reference

A `Crabtainerfile` is a text document containing instructions used to
assemble a container rootfs and configuration layout. It is consumed by
`crabtainer build` (see [CLI Reference](cli-reference.md#build--build-a-layout-from-a-crabtainerfile)).

## Supported Instructions

### `DOWNLOAD`

Downloads a container image from a registry (e.g. Docker Hub) and registers
it under a local alias in the image store.

```dockerfile
DOWNLOAD <IMAGE_REF> AS <ALIAS> [OVERRIDE]
```

* `<IMAGE_REF>` is a registry reference such as `ubuntu:24.04` or
  `ghcr.io/astral-sh/uv:python3.14-trixie-slim`.
* `OVERRIDE` re-downloads the image even if one with the same alias is
  already stored; without it, an existing image is reused.
* Alias names are sanitized for storage (`/` and `.` become `_`).

> [!NOTE]
> `image pull <IMAGE> <ALIAS>` is the CLI equivalent — see
> [CLI Reference](cli-reference.md#image--manage-images).

### FROM

Specifies the base image downloaded with `DOWNLOAD` and unpacks it into the
layout's rootfs. The base image's OCI config is read and its settings are
applied to the layout when the corresponding properties weren't already set:

* `entrypoint` + `cmd` → the container's default command
* `env` → default environment variables
* `working_dir` → default working directory

```Dockerfile
FROM <ALIAS>
```

### COPY

Copies files or directories from the host into the rootfs.

```Dockerfile
COPY <HOST_SRC> <CONTAINER_DST>
```

* `HOST_SRC` can be a path, a glob, or `*` meaning "copy everything from the
  build context" — files matched by `.crabtainerignore` are excluded (see
  below).
* `CONTAINER_DST` can be `~`, meaning "the working directory" (set by
  `WORKDIR`), or if no workdir is set, `/`.

### RUN

Executes a command inside the rootfs during the build process (after the
`FROM` step has unpacked the base image). Useful for installing packages,
creating users, or preparing application files.

```Dockerfile
RUN apt-get update && apt-get install -y curl
```

### CMD

Defines the default command and arguments to run when the container starts.
It can be overridden at runtime with `crabtainer run -c <cmd> [args...]`.

```Dockerfile
CMD /usr/bin/python3 app.py
```

### CPU_LIMIT

Sets the default maximum fractional CPU core limit for the image. The runtime
enforces it through the cgroup's `cpu.max` quota.

```Dockerfile
CPU_LIMIT 1.5
```

### MEMORY_LIMIT

Sets the default maximum memory limit for the image. Accepts a plain byte
count or the suffixes `b`, `k`, `m`, `g` (case-insensitive):

```Dockerfile
MEMORY_LIMIT 512m
MEMORY_LIMIT 2g
MEMORY_LIMIT 1024k
MEMORY_LIMIT 8b
```

### WORKDIR

Sets the working directory for the container. It is used as the default
destination for `COPY ... ~` and as the initial directory for the container's
process.

```Dockerfile
WORKDIR /var/www
```

## `.crabtainerignore`

During `COPY`, a `.crabtainerignore` file in the build context (the working
directory of `crabtainer build`) is honored, mirroring Docker's
`.dockerignore`:

* One glob pattern per line (supporting `*`, `?`, and `[...]` wildcards).
* `!` prefix re-includes a file/dir excluded by an earlier pattern.
* Blank lines and lines starting with `#` are ignored.

```text
# .crabtainerignore
.git
target/
*.log
build/*.tmp
!build/keep.txt
```

## Example

```dockerfile
DOWNLOAD debian:trixie AS debian

FROM debian

WORKDIR /app
COPY * ~

RUN apt-get update && apt-get install -y python3

CPU_LIMIT 1.0
MEMORY_LIMIT 512m

CMD python3 /app/app.py
```

Build it with:

```bash
sudo crabtainer build -f Crabtainerfile -t myapp-v1
```

See also the ready-made examples under [`examples/`](../examples) in the
repository.