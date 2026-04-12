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

Create the index definition file at `data/indexes/logs.index.yaml`. This file tells Edgewit how to parse and store your incoming log data.

```yaml
# data/indexes/logs.index.yaml

name: logs
description: "Application log events from edge services"

# The field that holds each document's primary timestamp.
# Must be declared below with type: datetime for retention to work.
timestamp_field: timestamp

# Strict about unknown fields to prevent schema bloat on disk-constrained devices
mode: drop_unmapped

# Automatically delete documents older than 7 days.
# Requires timestamp_field to be type: datetime with fast: true.
retention: 7d

# Compress segments for maximum storage efficiency on SD cards
compression: zstd

fields:
  timestamp:
    type: datetime
    indexed: true
    fast: true # Required for date_histogram aggregations and efficient retention queries

  level:
    type: keyword
    indexed: true
    fast: true # Required for terms aggregations

  service:
    type: keyword
    indexed: true

  message:
    type: text
    indexed: true
```

## 2. Create the Docker Compose File

Next, create the `docker-compose.yaml` file in the root of your project. We use the official GitHub Container Registry image and mount our local `./data` directory into the container.

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
      - ./data:/data
    restart: unless-stopped
    environment:
      # Disk limits — protects against SD card exhaustion
      EDGEWIT_MAX_INDEX_BYTES: "2GB"
      EDGEWIT_MAX_WAL_BYTES: "256MB"

      # Memory budget for Tantivy writer (safe default for Raspberry Pi 4)
      EDGEWIT_INDEX_MEMORY_MB: "50"

      # Optional: Uncomment to require an API key on all endpoints
      # EDGEWIT_API_KEY: "your-secret-key"

      # Optional: Disable dynamic index management to lock schema to YAML files only
      # EDGEWIT_API_INDEX_MANAGEMENT_ENABLED: "false"
```

_Note: Edgewit runs securely as a non-root user inside the container. If you encounter a `Permission denied (os error 13)` error when writing data, ensure your local `./data` directory is writable by the container (e.g., `chmod -R 777 ./data`)._

_Note on retention: with `retention: 7d` set, the background worker will delete documents with a `timestamp` older than 7 days every 5 minutes. Physical disk space is reclaimed when the compaction worker next merges segments._

## 3. Run the Stack

With the files in place, start Edgewit:

```bash
docker-compose up -d
```

Edgewit will start, read the `logs.index.yaml` from the mounted volume, and immediately initialize the `logs` index with the defined schema.

### Verify the Setup

Confirm the index was loaded:

```bash
curl http://localhost:9200/indexes/logs
```

### Ingest a Log Event

```bash
curl -X POST http://localhost:9200/logs/_doc \
  -H "Content-Type: application/json" \
  -d '{
    "timestamp": "2024-05-12T10:00:00Z",
    "level": "INFO",
    "service": "sensor-daemon",
    "message": "Sensor reading completed."
  }'
```

### Search

After a few seconds (for the indexer to commit), search your logs:

```bash
curl "http://localhost:9200/indexes/logs/_search?q=message:sensor"
```

You are now ready to start sending logs to your Edgewit instance!
