---
layout: default
title: Home
---

<section id="about" class="card" markdown="1">
## Why Edgewit?

Edgewit provides powerful full-text search and aggregations for local observability, offline log analytics, and IoT gateway diagnostics. It avoids the memory overhead and operational complexity of a centralized cloud solution by running efficiently on constrained hardware like the Raspberry Pi.

- **Edge-First:** Runs deterministically under 150MB of memory.
- **OpenSearch Compatible (Subset):** Drop-in replacement for basic log collection agents, implementing a focused subset of the API.
- **Crash-Resilient:** Custom WAL implementation built for slow SD cards.
</section>

<section id="quickstart" class="card" markdown="1">
## Quick Start

### 1. Start the Server

The easiest way to run Edgewit is via Docker:

```bash
docker run -p 9200:9200 -v edgewit_data:/app/data ghcr.io/viljami/edgewit:latest
```

Alternatively, compile from source:

```bash
git clone https://github.com/viljami/edgewit.git
cd edgewit
cargo run --release
```

### 2. Ingest Logs

Send a JSON document to the ingest endpoint. Edgewit automatically builds the schema.

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

### 3. Search

Query your logs using Lucene/OpenSearch syntax:

```bash
curl -X GET "http://localhost:9200/_search?q=_source.level:INFO"
```

</section>

<section id="configuration" class="card" markdown="1">
## Configuration & API

Check out the [Configuration Guide]({{ '/configuration/' | relative_url }}) and the [API Specification]({{ '/api/' | relative_url }}).

</section>
