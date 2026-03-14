---
layout: default
title: API Documentation
---

<div class="card" style="background-color: #ffffff; color: #333333;" markdown="1">
## Edgewit API

### Terminology: Indexes vs. Indices

In the Edgewit API and documentation, you will see the word **"indexes"** used as the plural for "index" (e.g., `/indexes/<name>`).

While "indices" is the traditional Latin plural, "indexes" is the standard plural in computer science when referring to database pointers or search indexes. Furthermore, using an explicit `/indexes/` path prefix (similar to Quickwit) prevents the root-level routing conflicts that OpenSearch often suffers from, keeping the API safely namespaced.

---

### Observability & Stats

Edgewit provides several OpenSearch-compatible observability endpoints to monitor the health and performance of your edge node:

- **`GET /_cat/indexes`**: Lists all active indexes along with their document counts and storage size approximations. Note that we deliberately use `/indexes` here instead of OpenSearch's `/indices` to remain consistent with our root CRUD endpoints.
- **`GET /_health`** or **`GET /_cluster/health`**: Returns a quick snapshot of the node's operational status.
- **`GET /_stats`**: Provides global search and ingestion metrics.
- **`GET /metrics`**: Exposes internal Prometheus-compatible metrics for scraping by systems like Grafana or Datadog.

---

<div id="redoc-container"></div>
<script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
<script>
    Redoc.init(
        "{{ '/openapi.json' | relative_url }}",
        {
            theme: {
                colors: {
                    primary: {
                        main: "#58a6ff",
                    },
                },
            },
        },
        document.getElementById("redoc-container"),
    );
</script>
</div>
