---
layout: default
title: Manual
---

# Edgewit Manual

Welcome to the Edgewit Manual. Here you'll find everything you need to get up and running with Edgewit, from a quick start guide to detailed configuration and API references.

<div class="card" markdown="1">
## 🚀 Quickstart

Get Edgewit running in seconds using Docker. This is the fastest way to start experimenting.

### 1. Start the Server

The easiest way to run Edgewit is via Docker. By default, Edgewit binds to port `9200` to maintain compatibility with the OpenSearch ecosystem:

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
curl -X GET "http://localhost:9200/indexes/logs/_search?q=_source.level:INFO"
```

</div>

<div class="card" markdown="1">
## 📚 Table of Contents

Explore the detailed sections of the manual to fully leverage Edgewit's capabilities in your edge environments:

- **[Configuration Guide]({{ '/configuration/' | relative_url }})**: Learn how to tune Edgewit for specific hardware profiles, manage memory, and optimize disk I/O.
- **[API Specification]({{ '/api/' | relative_url }})**: Discover the OpenAPI specification and learn about the available endpoints for indexing, searching, and cluster management.
- **[Example Setup]({{ '/example-setup/' | relative_url }})**: See a recommended Docker Compose setup with declarative index schemas.
- **[Security Model]({{ '/security/' | relative_url }})**: Understand Edgewit's approach to security, including API key authentication and network best practices.
- **[Benchmarks]({{ '/benchmark/' | relative_url }})**: Read about how Edgewit's performance compares to JVM-based alternatives on constrained hardware.
</div>
