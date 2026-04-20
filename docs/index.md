---
layout: default
title: Home
---

<section id="mission" class="card" markdown="1">
## Mission & Vision

**Mission:** To provide a robust, deterministic, and resource-efficient search and analytics engine tailored for constrained edge environments.

**Vision:** We believe observability at the edge shouldn't require compromising on capability or stability. Edgewit aims to replace heavy, centralized cloud observability stacks with a decentralized, edge-native solution that runs flawlessly on a Raspberry Pi or an embedded gateway.

</section>

<section id="about" class="card" markdown="1">
## Why Edgewit?

📦 **Container image:** [`ghcr.io/viljami/edgewit`](https://github.com/viljami/edgewit/pkgs/container/edgewit)

Edgewit provides powerful full-text search and aggregations for local observability, offline log analytics, and IoT gateway diagnostics. It avoids the memory overhead and operational complexity of a centralized cloud solution by running efficiently on constrained hardware like the Raspberry Pi.

- **Edge-First:** Runs deterministically under 150MB of memory.
- **OpenSearch Compatible (Subset):** Drop-in replacement for basic log collection agents, implementing a focused subset of the API.
- **Crash-Resilient:** Custom WAL implementation built for slow SD cards with deterministic startup recovery.
- **Declarative Indexing:** YAML-based index definitions with strict, drop-unmapped, or dynamic schema modes.
- **Single-Index Architecture:** One Tantivy index per logical index name — simple, predictable on-disk layout with no partition subdirectories.

Edgewit is intentionally single-node only—there is no cluster configuration or distributed mode, by design.

</section>

<section id="benchmarks" class="card" markdown="1">
## Benchmarks

Edgewit was benchmarked against OpenSearch in a constrained environment (1 vCPU, Edgewit 256MB RAM, OpenSearch 1.5GB RAM, 100k docs from OpenSearch Rally `http_logs`). The results:

| Metric                   | Edgewit         | OpenSearch     | Advantage    |
| :----------------------- | :-------------- | :------------- | :----------- |
| **Peak Memory Usage**    | ~25 MB          | >1 GB          | ~40x lighter |
| **Ingestion Throughput** | 86,200 docs/sec | 6,800 docs/sec | 12.7x faster |
| **Match All**            | 11,963 req/sec  | 379 req/sec    | 31x faster   |
| **Term Search**          | 13,684 req/sec  | 335 req/sec    | 41x faster   |
| **Aggregations**         | 15,912 req/sec  | 730 req/sec    | 21.8x faster |

See [Benchmarks](benchmark.md) for details.

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
