# Crabtainerfile Syntax Reference

A `Crabtainerfile` is a text document containing instructions used to assemble a container rootfs and configuration layout.

## Supported Instructions

### `DOWNLOAD`

Downloads a remote rootfs archive and registers it under a local alias.

```dockerfile
DOWNLOAD <URL> AS <ALIAS> [OVERRIDE]
```

### FROM

Specifies the base image or downloaded rootfs alias to build upon.

```Dockerfile
FROM <ALIAS_OR_IMAGE>
```

### COPY

Copies files or directories from the host path into the rootfs.

```Dockerfile
COPY <HOST_SRC> <CONTAINER_DST>
```

* HOST_SRC can be defined as '*' which means copy all files (exclude ignored by '.crabtainerignore' file) to CONTAINER_DST
* CONTAINER_DST can be defined as '~' which means destination will be workdir or if it is not specified, copies to '/'

### RUN

Executes a command inside the rootfs during the build process.

```Dockerfile
RUN apt-get update && apt-get install -y curl
```

### CMD

Defines the default command and arguments to run when the container starts.

```Dockerfile
CMD /usr/bin/python3 app.py
```

### CPU_LIMIT

Sets the default maximum fractional CPU core limit for the image.

```Dockerfile
CPU_LIMIT 1.5
```

### MEMORY_LIMIT

Sets the default maximum memory limit. Accepts suffixes: k, m, g.

```Dockerfile
MEMORY_LIMIT 512m
```

### WORKDIR

Sets the working directory for subsequent RUN and CMD instructions.

```Dockerfile
WORKDIR /var/www
```
