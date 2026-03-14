---
layout: default
title: Home
---

<section id="about" class="card" markdown="1">
## Why Edgewit?

📦 **Container image:** `ghcr.io/viljami/edgewit`

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

<section id="projects" class="card" markdown="1">
## Projects using Edgewit

- [ruuvi-home-lite](https://github.com/viljami/ruuvi-home-lite) - A browser PWA built for running and hosted on a Raspberry Pi 5. It connects to a local LAN Ruuvi Gateway to digest and present Ruuvi sensor data over time, including support for the latest Ruuvi air sensors.

</section>

<section id="news" class="card" markdown="1">
## Recent News

<ul style="list-style: none; padding: 0; margin-top: 1rem;">
  {% for post in site.posts limit:3 %}
    <li style="margin-bottom: 1rem; padding-bottom: 1rem; border-bottom: 1px solid var(--border);">
      <span style="color: #8b949e; font-size: 0.85em; display: block;">{{ post.date | date: "%B %-d, %Y" }}</span>
      <h3 style="margin-top: 0.2rem; margin-bottom: 0.5rem; font-size: 1.25em;">
        <a href="{{ post.url | relative_url }}">{{ post.title | escape }}</a>
      </h3>
      <p style="margin: 0; color: var(--text); opacity: 0.8; font-size: 0.95em;">{{ post.content | strip_html | truncatewords: 30 }}</p>
    </li>
  {% endfor %}
</ul>
</section>
