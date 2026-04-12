# Edgewit

**A lightweight, Rust-based search and analytics engine for edge environments.**

📦 **Container image:** `ghcr.io/viljami/edgewit`

Edgewit is inspired by [Quickwit](https://quickwit.io/) and built heavily upon [Tantivy](https://github.com/quickwit-oss/tantivy). It implements a focused, resource-efficient subset of the OpenSearch/Elasticsearch API designed specifically for constrained hardware, embedded systems, and single-board computers like the Raspberry Pi.

It provides powerful full-text search and aggregations for local observability, offline log analytics, and IoT gateway diagnostics—without the memory overhead or operational complexity of a centralized cloud solution.

## Features & Core Principles

- **Edge-First Architecture**: Runs deterministically within strict constraints (< 150MB resident memory target).
- **OpenSearch Compatible (Subset)**: Easily integrate with existing logging/observability agents by mimicking standard cluster, ingest, and search APIs. Note that it implements only a focused subset of the full OpenSearch API.
- **Rust Safety & Performance**: Written purely in Rust, minimizing binary footprint while offering massive concurrency and CPU safety.
- **Crash-Resilient Local WAL**: Custom Write-Ahead Log implementation explicitly designed to batch syncs and minimize unpredictable IOPS on edge SD-Cards, while ensuring 100% crash recovery.
- **Single-Index Architecture**: One Tantivy index per logical index name—no partition subdirectories, no multi-reader fan-out. Simple, predictable storage under `data/indexes/<name>/`.
- **Simple Deployment**: Single binary. Single container. No JVM, no external database dependencies.
- **Minimalist Security**: Designed for trusted network environments by default, with optional API Key authentication available via environment variables to maintain absolute minimum overhead.

## Benchmarks

Edgewit was built specifically to run in environments where JVM-based systems fail. In our edge-simulated benchmark (1 vCPU, constrained RAM), Edgewit completely outperformed a tuned OpenSearch node.

**Test Environment:** 1 vCPU. Edgewit (256MB RAM Limit) vs OpenSearch (1.5GB RAM Limit).
**Dataset:** 100,000 document subset of the OpenSearch Rally `http_logs` dataset.

| Metric                   | Edgewit                    | OpenSearch            | Advantage    |
| :----------------------- | :------------------------- | :-------------------- | :----------- |
| **Peak Memory Usage**    | **~25 MB**                 | > 1 GB (Thrashing)    | ~40x lighter |
| **Ingestion Throughput** | **~300,000 docs/sec**      | 0 docs/sec (Crashed)  | Infinite     |
| **Search: Match All**    | **3,402 req/sec** (2.95ms) | 255 req/sec (43.62ms) | 13x faster   |
| **Search: Term Query**   | **6,558 req/sec** (2.40ms) | 63 req/sec (159.18ms) | 104x faster  |
| **Search: Aggregation**  | **1,730 req/sec** (6.08ms) | 108 req/sec (95.63ms) | 16x faster   |

For the full detailed results and methodology, see [BENCHMARK_PLAN.md](BENCHMARK_PLAN.md) or our documentation site.

## Quick Start

### Running Locally (Native)

You will need the Rust toolchain installed.

```bash
# Clone the repository
git clone https://github.com/viljami/edgewit.git
cd edgewit

# Run the server locally
cargo run
```

Edgewit will automatically bind to `0.0.0.0:9200` by default.

### Running via Docker

A fully functional `docker-compose.yaml` and `Dockerfile` are provided. The multi-stage build creates an extremely thin Debian-based image running as a non-root user.

You can run the pre-built container image directly from GitHub Container Registry:

```bash
docker run -p 9200:9200 -v edgewit_data:/app/data ghcr.io/viljami/edgewit:latest
```

Or build it locally with Docker Compose:

```bash
docker compose up --build
```

### Ingestion Example

Send a single document to an index, or use the bulk API for high-throughput ingestion.

```bash
# Single document
curl -X POST http://localhost:9200/my-edge-logs/_doc \
  -H "Content-Type: application/json" \
  -d '{
    "timestamp": "2024-05-12T10:00:00Z",
    "level": "INFO",
    "message": "System booted successfully.",
    "sensor_id": "rasp-01"
  }'

# Bulk ingestion (NDJSON)
curl -X POST http://localhost:9200/_bulk \
  -H "Content-Type: application/x-ndjson" \
  --data-binary '
{"index":{"_index":"my-edge-logs"}}
{"timestamp":"2024-05-12T10:00:01Z","level":"WARN","message":"Disk usage high."}
'
```

### Search Example

Once you have ingested some data, wait a few seconds for the background indexer to commit the segment, then search:

```bash
curl -X GET "http://localhost:9200/indexes/my-edge-logs/_search?q=message:booted"
```

## Monitoring & Observability

Edgewit provides a built-in Prometheus-compatible metrics endpoint at `GET /metrics`. You can configure a Prometheus or OpenTelemetry scraper to periodically collect these stats.

```bash
curl http://localhost:9200/metrics
```

Available metrics include:

- `edgewit_ingest_requests_total`
- `edgewit_ingest_bytes_total`
- `edgewit_search_requests_total`
- `edgewit_search_latency_seconds`
- `edgewit_index_docs_total`
- `edgewit_index_segments_total`

## Configuration

Edgewit is designed to be configured entirely via environment variables, adhering to the 12-factor app methodology. This makes it incredibly easy to manage within container orchestrators.

| Environment Variable            | Default Value | Description                                                                                                                               |
| :------------------------------ | :------------ | :---------------------------------------------------------------------------------------------------------------------------------------- |
| `RUST_LOG`                      | `info`        | Sets the logging level. Uses standard `tracing` EnvFilter syntax (e.g., `info`, `edgewit=debug`).                                         |
| `EDGEWIT_PORT`                  | `9200`        | The port the HTTP API binds to.                                                                                                           |
| `EDGEWIT_API_KEY`               | `None`        | Enables HTTP header authentication (`Authorization: Bearer <key>`) when set. Highly recommended for shared environments.                  |
| `EDGEWIT_DATA_DIR`              | `./data`      | Directory where Tantivy indexes and WAL files are stored.                                                                                 |
| `EDGEWIT_MAX_INDEX_BYTES`       | `1GB`         | Maximum disk size for searchable indexes. Exceeding this logs a warning. Supports human-readable suffixes (`KB`, `MB`, `GB`).             |
| `EDGEWIT_MAX_WAL_BYTES`         | `512MB`       | Maximum disk size for uncommitted WAL files. Exceeding this triggers emergency WAL pruning to prevent disk exhaustion. Supports suffixes. |
| `EDGEWIT_INDEX_MEMORY_MB`       | `30`          | Memory budget in MB for each Tantivy IndexWriter. Lower values limit RAM usage but may trigger more frequent disk commits.                |
| `EDGEWIT_CHANNEL_BUFFER`        | `10000`       | Number of events to buffer in memory channels before blocking ingestion.                                                                  |
| `EDGEWIT_SEARCH_THREADS`        | `1`           | Number of Rayon threads allocated for search queries. Lower values prevent CPU starvation on embedded multi-core chips.                   |
| `EDGEWIT_DOCSTORE_CACHE_BLOCKS` | `20`          | Number of uncompressed document blocks to keep in RAM during search. Lower values limit memory overhead.                                  |
| `EDGEWIT_MERGE_MIN_SEGMENTS`    | `10`          | Minimum segment count before background compaction triggers.                                                                              |
| `EDGEWIT_COMMIT_INTERVAL_SECS`  | `5`           | Time interval for the indexer's adaptive commit.                                                                                          |
| `EDGEWIT_COMMIT_INTERVAL_DOCS`  | `10000`       | Document count limit for the indexer's adaptive commit.                                                                                   |

## API Documentation

The API documentation is generated directly from the source code using `utoipa` to guarantee absolute accuracy.

To generate the current OpenAPI specs (and view them via Redoc):

```bash
cargo run --bin generate_openapi
```

This will output `docs/openapi.json` which is automatically picked up by the `docs/index.html` file.

_If configured, Github Actions automatically pushes these documents to Github Pages on every commit._

## Security

Edgewit is designed primarily for trusted edge environments such as embedded systems, internal networks, or home lab infrastructure. By default, it runs with a **Layer 1 Trusted Network Model**, binding without authentication overhead to maximize ingest performance and simplify local deployments.

For environments requiring access control, you can enable a **Layer 2 Optional API Key Authentication** simply by setting the `EDGEWIT_API_KEY` environment variable. This mandates an `Authorization: Bearer <key>` header on all requests while avoiding the bloated overhead of a full internal user database or session manager.

For full details and guidelines on public deployment architectures, read the [Security Model Documentation](SECURITY_PLAN.md).

## Architecture

Edgewit is separated into specialized asynchronous actors to ensure peak HTTP performance while safely handling slow block-storage mediums:

1. **HTTP Ingest API (Axum):** Validates the JSON payload and pushes it immediately to an in-memory channel.
2. **Write-Ahead Log (WAL) Thread:** Adaptive batching engine. Waits for incoming events, frames them into binary blobs, calculates a CRC32 checksum, and pushes massive contiguous writes to disk via a single OS `sync_data`. This is the secret to getting ~5k writes/sec on cheap MicroSD cards.
3. **Indexer Engine (Tantivy):** A background loop consumes the synced WAL events and writes them into a single Tantivy index per logical index name (stored at `data/indexes/<name>/`). Segments are committed periodically with the WAL offset embedded in the commit payload to ensure seamless crash recovery.
4. **Compaction Worker:** Periodically merges small Tantivy segments within each index directory to bound open file handles and keep search performance stable.
5. **Retention Worker:** Every 5 minutes, iterates all indexes with a `retention:` field set, computes a cutoff timestamp, and sends a purge command to the Indexer Engine. The indexer stages a Tantivy `delete_query` range deletion on the `timestamp` fast field, which is flushed to disk on the next regular commit. Also monitors total disk usage and prunes uncommitted WAL files when over the configured size limit.

## Project Milestones

- ✅ **M0 Project Foundation:** Runnable system, repository layout, container build.
- ✅ **M1 Ingestion Pipeline:** Custom adaptive WAL, durable persistence, HTTP ingest APIs.
- ✅ **M2 Indexing Engine:** Tantivy integration, dynamic JSON schema, WAL-replay on startup.
- ✅ **M3 Search Engine:** Implement `/_search` with query parsing and sorting.
- ✅ **M4 Aggregation Engine:** Analytical queries natively on the edge.
- ✅ **M5 Segment Management:** Compaction, WAL rotation, and disk usage limits.
- ✅ **M6 Edge Optimization:** Memory budgeting, search threads, cache tuning.
- ✅ **M7 OpenSearch Compatibility:** OpenSearch compatible API mappings.
- ✅ **M8 Observability:** Metrics endpoint and Prometheus compatibility.
- ✅ **M9 Simplification:** Single-index architecture — one Tantivy index per logical index, eliminating partition-based storage complexity.

_(See `PROJECT.md` for a full breakdown of the project vision)._

## Projects using Edgewit

- [ruuvi-home-lite](https://github.com/viljami/ruuvi-home-lite) - A browser PWA built for running and hosted on a Raspberry Pi 5. It connects to a local LAN Ruuvi Gateway to digest and present Ruuvi sensor data over time, including support for the latest Ruuvi air sensors.

## Contributing

Pull requests, issues, and feature suggestions are highly encouraged! When writing tests, please refer to the existing inline snapshots built with `axum-test` and `insta`.

```bash
# Run the test suite
cargo test

# If tests fail due to intentionally modified API outputs, update snapshots:
cargo insta review
```

## License

Edgewit is licensed under the MIT License. See [LICENSE](LICENSE) for more information.
