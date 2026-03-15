---
layout: default
title: Example Setup
---

# Recommended Setup: Edge Logging Example

This guide demonstrates a recommended project setup for Edgewit using Docker Compose. In this example, we will configure Edgewit to run as a local edge logging server, utilizing a pre-defined index schema mounted directly into the container.

Using a pre-defined index definition file (`.index.yaml`) ensures that your schema is highly deterministic and recovers instantly upon startup, which is a best practice for edge environments.

## Project Structure

Create a new directory for your project and set up the following file structure:

```text
my-edgewit-project/
├── docker-compose.yaml
└── data/
    └── indexes/
        └── logs.index.yaml
```

_Note: Edgewit expects index definitions to be placed in the `indexes/` subdirectory of its data folder._

## 1. Define the Index Schema

Create the index definition file at `data/indexes/logs.index.yaml`. This file tells Edgewit how to parse, store, partition, and retain your incoming log data.

```yaml
# data/indexes/logs.index.yaml

name: logs
description: "Application log events from edge services"

# Use the timestamp field for time partitioning and retention
timestamp_field: timestamp

# Only index fields explicitly defined below; drop unmapped fields to save space
mode: drop_unmapped

# Partition data daily
partition: daily

# Automatically delete logs older than 7 days
retention: 7d

# Compress segments using zstd for optimal storage
compression: zstd

fields:
  timestamp:
    type: datetime
    indexed: true
    fast: true

  level:
    type: keyword
    indexed: true
    fast: true

  service:
    type: keyword
    indexed: true

  message:
    type: text
    indexed: true
```

## 2. Create the Docker Compose File

Next, create the `docker-compose.yaml` file in the root of your project. We will use the official GitHub Container Registry image (`ghcr.io/viljami/edgewit`) and mount our local `./data` directory into the container's `/app/data` directory.

```yaml
# docker-compose.yaml

version: "3.8"

services:
  edgewit:
    image: ghcr.io/viljami/edgewit:latest
    container_name: edgewit
    ports:
      - "9200:9200"
    volumes:
      # Mount the local data directory (containing our logs.index.yaml)
      # to the container's data directory.
      - ./data:/app/data
    restart: unless-stopped
    environment:
      # Optional: Disable dynamic index creation via API to lock down the schema
      # EDGEWIT_API_INDEX_MANAGEMENT_ENABLED: "false"
```

## 3. Run the Stack

With the files in place, you can start your Edgewit instance by running:

```bash
docker-compose up -d
```

Edgewit will start up, read the `logs.index.yaml` file from the mounted volume, and immediately initialize the `logs` index with the defined schema and a 7-day retention policy.

### Verify the Setup

You can verify that the index was successfully loaded by querying the `/indexes` endpoint:

```bash
curl http://localhost:9200/indexes/logs
```

You are now ready to start sending logs to your Edgewit instance at `http://localhost:9200/logs/_doc`!
