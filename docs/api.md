---
layout: default
title: API Documentation
---

<div class="card" markdown="1">
## Edgewit API

### Terminology: Indexes vs. Indices

In the Edgewit API and documentation, you will see the word **"indexes"** used as the plural for "index" (e.g., `/indexes/<name>`).

While "indices" is the traditional Latin plural, "indexes" is the standard plural in computer science when referring to database pointers or search indexes. Furthermore, using an explicit `/indexes/` path prefix (similar to Quickwit) prevents the root-level routing conflicts that OpenSearch often suffers from, keeping the API safely namespaced.

---

### API Versioning & Compatibility

You might notice that Edgewit does not use path-based versioning (e.g., `/api/v1/_bulk`). This is an intentional design choice to maintain **drop-in compatibility** with the OpenSearch and Elasticsearch ecosystem.

Standard log shippers (like Vector, Fluent Bit, or Filebeat) expect to send payloads to root-level endpoints like `/_bulk`. Altering these paths would break out-of-the-box integrations. If breaking API changes are ever required in the future, versioning will be handled via HTTP headers (e.g., `Accept: application/vnd.edgewit+json; version=2`) to ensure legacy edge deployments continue to function without interruption.

---

### Observability & Stats

Edgewit provides several OpenSearch-compatible observability endpoints to monitor the health and performance of your edge node:

- **`GET /_cat/indexes`**: Lists all active indexes along with their document counts and storage size approximations. Note that we deliberately use `/indexes` here instead of OpenSearch's `/indices` to remain consistent with our root CRUD endpoints.
- **`GET /_health`** or **`GET /_cluster/health`**: Returns a quick snapshot of the node's operational status.
- **`GET /_stats`**: Provides search and ingestion metrics.
- **`GET /metrics`**: Exposes internal Prometheus-compatible metrics for scraping by systems like Grafana or Datadog.

### Query parsing and `_dynamic` (dynamic mode)

When an index is configured with `mode: dynamic`, Edgewit creates a dedicated `_dynamic` text field in the Tantivy schema. Any unmapped fields from incoming JSON documents are concatenated into a textual representation and stored in `_dynamic`. The HTTP query parser will prefer this `_dynamic` field for term-style queries when it is available in the index schema.

Practical effects:

- Simple term queries that reference an unmapped field (for example `q=message:hello`) will match if `_dynamic` contains that token.
- If your index contains an explicitly mapped field (e.g., `message` defined in `fields`), exact behavior for that mapped field remains intact; `_dynamic` exists to provide a predictable catch-all for unmapped content in dynamic mode.
- Because `_dynamic` is a tokenized text field, it is suited for simple term and full-text-style matching, not for structured JSON lookups.

### Metrics & e2e expectations

The `/metrics` endpoint renders Prometheus text exposition output. To make automated tests and scrapers deterministic, Edgewit registers a small set of commonly expected metric names at startup (with zero or empty initial values) so that they are visible even before any ingestion/search traffic:

- `edgewit_ingest_requests_total`
- `edgewit_ingest_bytes_total`
- `edgewit_search_requests_total`
- `edgewit_search_latency_seconds`
- `edgewit_index_docs_total`
- `edgewit_index_segments_total`

The included end-to-end test scripts (`scripts/e2e/`) assert several behaviors that you should expect from a running node:

- Cluster endpoints (`/`, `/version`, `/_health`) respond with HTTP 200 and basic metadata.
- The `_stats` and `_cat/indexes` endpoints reflect document counts after ingest.
- The `/metrics` endpoint is reachable and contains the expected metric names above.
- Index lifecycle operations (`PUT /indexes/<name>`, `GET /indexes/<name>`, `DELETE /indexes/<name>`) work as documented.
- Ingest endpoints (`POST /<index>/_doc`, `POST /_bulk`) return the expected HTTP status codes and responses for single and bulk ingestion.
- Search and aggregation endpoints (`/{index}/_search`) return hits and aggregation buckets consistent with the index schema and the documents ingested. Note: some e2e search checks target the `_dynamic` catch‑all behavior for dynamic-mode tests; other checks use explicitly mapped fields for deterministic assertions.

When upgrading or running the e2e suite against persisted data, ensure any on-disk data created by older versions is migrated or reindexed if it relied on prior `_dynamic` encodings. For ephemeral test volumes, removing and recreating the test volume is the fastest way to guarantee compatibility with the new behavior:

```bash
docker volume rm edgewit-e2e-persist-vol
```

</div>

<div class="card" style="background-color: #ffffff; color: #333333;">
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
