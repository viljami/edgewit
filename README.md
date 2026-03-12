# Edgewit

**A lightweight, Rust-based search and analytics engine for edge environments.**

Edgewit is inspired by [Quickwit](https://quickwit.io/) and built heavily upon [Tantivy](https://github.com/quickwit-oss/tantivy). It implements a focused, resource-efficient subset of the OpenSearch/Elasticsearch API designed specifically for constrained hardware, embedded systems, and single-board computers like the Raspberry Pi.

It provides powerful full-text search and aggregations for local observability, offline log analytics, and IoT gateway diagnostics—without the memory overhead or operational complexity of a centralized cloud solution.

## Features & Core Principles

- **Edge-First Architecture**: Runs deterministically within strict constraints (< 150MB resident memory target).
- **OpenSearch Compatible (Subset)**: Easily integrate with existing logging/observability agents by mimicking standard cluster, ingest, and search APIs. Note that it implements only a focused subset of the full OpenSearch API.
- **Rust Safety & Performance**: Written purely in Rust, minimizing binary footprint while offering massive concurrency and CPU safety.
- **Crash-Resilient Local WAL**: Custom Write-Ahead Log implementation explicitly designed to batch syncs and minimize unpredictable IOPS on edge SD-Cards, while ensuring 100% crash recovery.
- **Simple Deployment**: Single binary. Single container. No JVM, no external database dependencies.

## Quick Start

### Running Locally (Native)

You will need the Rust toolchain installed.

```bash
# Clone the repository
git clone https://github.com/yourusername/edgewit.git
cd edgewit

# Run the server locally
cargo run
```

Edgewit will automatically bind to `0.0.0.0:9200` by default.

### Running via Docker

A fully functional `docker-compose.yaml` and `Dockerfile` are provided. The multi-stage build creates an extremely thin Debian-based image running as a non-root user.

```bash
docker compose up --build
```

### Ingestion Example

Edgewit currently supports standard document ingestion (bulk API support coming soon).

```bash
curl -X POST http://localhost:9200/my-edge-logs/_doc \
  -H "Content-Type: application/json" \
  -d '{
    "timestamp": "2024-05-12T10:00:00Z",
    "level": "INFO",
    "message": "System booted successfully.",
    "sensor_id": "rasp-01"
  }'
```

### Search Example

Once you've ingested some data, wait a few seconds for the background indexer to commit the segment, then you can search for it using the `/_search` endpoint.

```bash
curl -X GET "http://localhost:9200/_search?q=_source.message:booted"
```

## Configuration

Edgewit is designed to be configured entirely via environment variables, adhering to the 12-factor app methodology. This makes it incredibly easy to manage within container orchestrators.

| Environment Variable            | Default Value | Description                                                                                                                                 |
| :------------------------------ | :------------ | :------------------------------------------------------------------------------------------------------------------------------------------ |
| `EDGEWIT_PORT`                  | `9200`        | The port the HTTP API binds to.                                                                                                             |
| `EDGEWIT_DATA_DIR`              | `./data`      | Directory where Tantivy segments and WAL files are stored.                                                                                  |
| `EDGEWIT_MAX_INDEX_BYTES`       | `1GB`         | Maximum disk size for the searchable index. Exceeding this triggers retention pruning. Supports human-readable suffixes (`KB`, `MB`, `GB`). |
| `EDGEWIT_MAX_WAL_BYTES`         | `512MB`       | Maximum disk size for uncommitted WAL files. Exceeding this triggers emergency WAL pruning to prevent disk exhaustion. Supports suffixes.   |
| `EDGEWIT_INDEX_MEMORY_MB`       | `30`          | Memory budget in MB for the Tantivy IndexWriter. Lower values limit RAM usage but may trigger more frequent disk commits.                   |
| `EDGEWIT_CHANNEL_BUFFER`        | `10000`       | Number of events to buffer in memory channels before blocking ingestion.                                                                    |
| `EDGEWIT_SEARCH_THREADS`        | `1`           | Number of Rayon threads allocated for resolving search queries. Lower values prevent CPU starvation on embedded multi-core chips.           |
| `EDGEWIT_DOCSTORE_CACHE_BLOCKS` | `20`          | Number of uncompressed document blocks to keep in RAM during search operations. Lower values limit memory overhead.                         |
| `EDGEWIT_MERGE_MIN_SEGMENTS`    | `10`          | Minimum number of segments required before triggering a background compaction. Higher values reduce write amplification.                    |
| `EDGEWIT_COMMIT_INTERVAL_SECS`  | `5`           | Time interval constraint for the background indexer's adaptive batching.                                                                    |
| `EDGEWIT_COMMIT_INTERVAL_DOCS`  | `10000`       | Document limit constraint for the background indexer's adaptive batching.                                                                   |

## API Documentation

The API documentation is generated directly from the source code using `utoipa` to guarantee absolute accuracy.

To generate the current OpenAPI specs (and view them via Redoc):

```bash
cargo run --bin generate_openapi
```

This will output `docs/openapi.json` which is automatically picked up by the `docs/index.html` file.

_If configured, Github Actions automatically pushes these documents to Github Pages on every commit._

## Architecture

Edgewit is separated into specialized asynchronous actors to ensure peak HTTP performance while safely handling slow block-storage mediums:

1. **HTTP Ingest API (Axum):** Validates the JSON payload and pushes it immediately to an in-memory channel.
2. **Write-Ahead Log (WAL) Thread:** Adaptive batching engine. Waits for incoming events, frames them into binary blobs, calculates a CRC32 checksum, and pushes massive contiguous writes to disk via a single OS `sync_data`. This is the secret to getting ~5k writes/sec on cheap MicroSD cards.
3. **Indexer Engine (Tantivy):** A background loop consumes the synced WAL events, pushes them into a dynamic JSON-schema memory buffer, and commits `.mmap` segment files periodically. The offset of the WAL is injected into Tantivy's commit payload to ensure seamless disaster recovery!

## Project Milestones

- ✅ **M0 Project Foundation:** Runnable system, repository layout, container build.
- ✅ **M1 Ingestion Pipeline:** Custom adaptive WAL, durable persistence, HTTP ingest APIs.
- ✅ **M2 Indexing Engine:** Tantivy integration, dynamic JSON schema, WAL-replay on startup.
- ✅ **M3 Search Engine:** Implement `/_search` with query parsing and sorting.
- ✅ **M4 Aggregation Engine:** Analytical queries natively on the edge.
- ✅ **M5 Segment Management:** Compaction, WAL rotation, and disk usage limits.

_(See `PROJECT.md` for a full breakdown of the project vision)._

## Contributing

Pull requests, issues, and feature suggestions are highly encouraged! When writing tests, please refer to the existing inline snapshots built with `axum-test` and `insta`.

```bash
# Run the test suite
cargo test

# If tests fail due to intentionally modified API outputs, update snapshots:
cargo insta review
```
