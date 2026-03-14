---
layout: default
title: Configuration
---

# Edgewit Configuration Guide

Edgewit is built specifically for edge environments, such as Raspberry Pi devices and embedded systems. To support a wide variety of hardware profiles—from constrained 512MB devices to more capable 8GB edge gateways—Edgewit is configured entirely via environment variables.

This guide details all available configuration parameters, with a special focus on the Edge Optimization features introduced in Milestone 6 (M6).

---

## General Configuration

| Variable           | Default  | Description                                                                                                                                               |
| :----------------- | :------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `RUST_LOG`         | `info`   | Sets the logging level. Uses standard `tracing` EnvFilter syntax (e.g., `info`, `edgewit=debug`).                                                         |
| `EDGEWIT_PORT`     | `9200`   | The TCP port the HTTP API binds to. Defaults to 9200 for basic OpenSearch compatibility.                                                                  |
| `EDGEWIT_DATA_DIR` | `./data` | The directory where Tantivy index segments and Write-Ahead Log (WAL) files are persisted. Ensure the application has read/write permissions to this path. |

---

## Retention & Disk Management

To ensure Edgewit operates predictably without exhausting the local disk (often an SD card in edge deployments), strict size thresholds can be configured.

| Variable                  | Default | Description                                                                                                                                                               |
| :------------------------ | :------ | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `EDGEWIT_MAX_INDEX_BYTES` | `1GB`   | Maximum allowed disk usage for the searchable Tantivy index. Background workers will prune older segments to stay under this limit. Supports suffixes (`KB`, `MB`, `GB`). |
| `EDGEWIT_MAX_WAL_BYTES`   | `512MB` | Maximum allowed disk space for uncommitted WAL files. If this threshold is hit, older WAL files will be rotated out forcefully. Supports suffixes.                        |

---

## Edge Optimization (Memory & CPU)

The following parameters were introduced specifically to tune Edgewit for constrained architectures. They control the delicate balance between memory consumption, CPU utilization, and disk I/O.

### `EDGEWIT_INDEX_MEMORY_MB`

- **Default:** `30`
- **Description:** Defines the memory budget (in Megabytes) allocated to the Tantivy `IndexWriter`.
- **Tuning Advice:**
  - Lowering this value (e.g., `15`) strictly caps memory usage during ingestion spikes, making it safer for 512MB devices. However, this forces Tantivy to flush segments to disk more frequently, increasing disk write amplification.
  - Raising this value (e.g., `100`) reduces disk writes (saving SD card lifespan) and allows larger, more optimal segments to be built in memory, but requires more available RAM.

### `EDGEWIT_CHANNEL_BUFFER`

- **Default:** `10000`
- **Description:** The size of the in-memory asynchronous channels (MPSC queues) passing events between the HTTP ingestion API, the WAL writer, and the Tantivy indexer.
- **Tuning Advice:**
  - If your device experiences massive, sudden bursts of ingestion traffic, this buffer absorbs the spike without rejecting HTTP requests.
  - However, a buffer of 10,000 large JSON documents can consume significant RAM. On highly constrained devices, reducing this to `1000` or `5000` limits peak memory footprint at the cost of earlier HTTP backpressure (returning `503 Service Unavailable` or delaying responses during spikes).

### `EDGEWIT_SEARCH_THREADS`

- **Default:** `1`
- **Description:** The number of Rayon worker threads allocated to the search engine to process `/_search` queries in parallel.
- **Tuning Advice:**
  - In standard cloud-based search engines, this defaults to the number of logical CPU cores. On an edge device acting primarily as an ingestion gateway, dedicating all cores to a search query can starve the system, causing ingestion drops or system instability.
  - Leaving this at `1` guarantees deterministic, single-threaded search execution. If you are deploying on a Raspberry Pi 4/5 and doing heavy local analytics, you can safely bump this to `2` or `3` to improve query latency.

### Write Reduction & Caching Options

### `EDGEWIT_DOCSTORE_CACHE_BLOCKS`

- **Default:** `20`
- **Description:** Number of uncompressed document blocks to keep in RAM during search operations.
- **Tuning Advice:** Lowering this (e.g., `5` or `10`) strictly limits search-time memory overhead on RAM-constrained devices, but increases disk reads for queries that return many `_source` documents.

### `EDGEWIT_MERGE_MIN_SEGMENTS`

- **Default:** `10`
- **Description:** Minimum number of segments required before triggering a background compaction (merge).
- **Tuning Advice:** Higher values (e.g., `20`) reduce write amplification and save SD card wear by merging less often, at the cost of slightly slower search performance due to more open segment files.

### `EDGEWIT_COMMIT_INTERVAL_SECS` & `EDGEWIT_COMMIT_INTERVAL_DOCS`

- **Default:** `5` (Seconds) / `10000` (Documents)
- **Description:** Controls the background indexer's adaptive batching constraints. The indexer will flush a new segment to disk whenever either threshold is hit.
- **Tuning Advice:** On unreliable power, keep these low for faster search visibility. On stable edge devices where saving write cycles is paramount, raise these to batch more events in memory before flushing to disk.

---

## Example Hardware Profiles

### Profile: Minimal (e.g., Raspberry Pi Zero 2 W, 512MB RAM)

Prioritize low memory usage, accept higher disk writes and potential backpressure.

```bash
export EDGEWIT_INDEX_MEMORY_MB=15
export EDGEWIT_CHANNEL_BUFFER=2000
export EDGEWIT_SEARCH_THREADS=1
export EDGEWIT_MAX_INDEX_BYTES=500MB
export EDGEWIT_DOCSTORE_CACHE_BLOCKS=5
export EDGEWIT_MERGE_MIN_SEGMENTS=15
```

### Profile: Balanced Edge Gateway (e.g., Raspberry Pi 4, 2GB RAM)

Good balance of memory usage and reduced SD card wear.

```bash
export EDGEWIT_INDEX_MEMORY_MB=50
export EDGEWIT_CHANNEL_BUFFER=10000
export EDGEWIT_SEARCH_THREADS=2
export EDGEWIT_MAX_INDEX_BYTES=2GB
export EDGEWIT_DOCSTORE_CACHE_BLOCKS=20
export EDGEWIT_MERGE_MIN_SEGMENTS=10
```

### Profile: Heavy Analytics Node (e.g., Raspberry Pi 5, 8GB RAM, NVMe)

Prioritize batching efficiency and fast analytics; assume fast storage and plenty of RAM.

```bash
export EDGEWIT_INDEX_MEMORY_MB=250
export EDGEWIT_CHANNEL_BUFFER=50000
export EDGEWIT_SEARCH_THREADS=4
export EDGEWIT_MAX_INDEX_BYTES=10GB
export EDGEWIT_DOCSTORE_CACHE_BLOCKS=100
export EDGEWIT_MERGE_MIN_SEGMENTS=8
export EDGEWIT_COMMIT_INTERVAL_SECS=30
```
