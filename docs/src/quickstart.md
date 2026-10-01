# Quick Start Guide

This guide will walk you through building your first layout and running a container.

## Prerequisites

- Linux (Kernel 5.x or newer recommended)
- `cgroups v2` enabled
- Root privileges for namespace creation

## Steps

### Step 1: Initialize System Directories

Before running any containers, set up Crabtainer's runtime paths and network bridges:

```bash
sudo crabtainer system init
```

### Step 2: Create a Crabtainerfile

Create a file named Crabtainerfile in your working directory:

```Dockerfile
DOWNLOAD alpine:latest AS alpine
FROM alpine
WORKDIR /app
COPY ./app.sh ~
CPU_LIMIT 1.0
MEMORY_LIMIT 512m
CMD /app/app.sh
```

### Step 3: Build the Layout

```bash
crabtainer build --tag myapp-v1
```

### Step 4: Run the Container

```bash
# Run interactively
crabtainer run myapp-v1

# Run in background with resource limits
crabtainer run -d --name production-app -C 2.0 -M 1024m myapp-v1
```
