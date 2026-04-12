---
layout: default
title: Index Schema & Definition
---

# Edgewit Index Schema & Definition

Edgewit is designed to provide highly deterministic, fast, and stable search capabilities in edge environments. To achieve this, Edgewit relies on explicit **Index Definitions** (schemas).

Unlike heavy cloud-native search engines that rely on complex distributed cluster state, Edgewit stores index definitions locally as simple, human-readable YAML files. These files act as the absolute source of truth upon startup, ensuring that if an edge device loses power or connectivity, it recovers instantly and reliably.

---

## The Index Definition File

An index definition maps your JSON documents to Tantivy's underlying high-performance search schema. It dictates what fields are searchable, what fields can be aggregated, and how data is managed on disk.

_(Note on terminology: In Edgewit, we use **"indexes"** rather than "indices" as the plural of index. This aligns with modern computer science standards and, by explicitly using an `/indexes/` path prefix in the API, we avoid the root-level routing conflicts that systems like OpenSearch often experience.)_

### File Location and Naming

Index definitions are stored in the `/indexes/` subdirectory of your configured `EDGEWIT_DATA_DIR` (default: `./data/indexes/`).

Files must be named using the pattern: `<index-name>.index.yaml`. For example, the definition for the `logs` index must be named `logs.index.yaml`.

---

## Basic Example

Here is a complete example of a typical `logs.index.yaml` file:

```yaml
name: logs
description: "Application log events from edge services"

# The field that holds each document's primary timestamp.
# Must be declared below with type: datetime for retention to work.
timestamp_field: timestamp

# Enforcement mode: 'strict', 'drop_unmapped', or 'dynamic'
mode: drop_unmapped

# Automatically delete documents older than 7 days.
# Requires timestamp_field to be type: datetime with fast: true.
retention: 7d

# Compression algorithm for Tantivy segments
compression: zstd

fields:
  timestamp:
    type: datetime
    indexed: true
    fast: true # fast: true is required for efficient retention queries

  level:
    type: keyword
    indexed: true
    fast: true

  service:
    type: keyword
    indexed: true

  message:
    type: text
    indexed: true

  response_time_ms:
    type: float
    indexed: true
    fast: true
```

---

## Root Level Configuration

| Property          | Required | Description                                                                                                                                                                                                                                                                                                                                                                             |
| :---------------- | :------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`            | **Yes**  | The unique name of the index. Must match the API endpoint (e.g., `PUT /indexes/logs`) and the filename prefix.                                                                                                                                                                                                                                                                          |
| `description`     | No       | A human-readable description of the index's purpose.                                                                                                                                                                                                                                                                                                                                    |
| `timestamp_field` | No       | The JSON field that holds each document's primary timestamp. **Required for `retention` enforcement** — the named field must be declared in `fields` with `type: datetime`. Add `fast: true` for efficient retention queries and date aggregations. (Default: `timestamp`).                                                                                                             |
| `mode`            | No       | Defines how Edgewit handles JSON fields not explicitly defined in the schema. Options: `strict`, `drop_unmapped`, `dynamic`. (Default: `dynamic`).                                                                                                                                                                                                                                      |
| `partition`       | No       | Schema annotation declaring an intended time-bucketing strategy. Stored and validated but not used for physical storage layout. Options: `none`, `daily`, `hourly`, `monthly`. (Default: `none`).                                                                                                                                                                                       |
| `retention`       | No       | Actively enforced time-based data expiration. Every 5 minutes, all documents older than this duration are deleted via a Tantivy range query on `timestamp_field`. **Requires `timestamp_field` to be declared with `type: datetime`** (and `fast: true` for efficient execution). Format: number + case-sensitive unit (`s`, `m`, `h`, `d`, `w`, `M`, `y`). Example: `7d`, `30d`, `1y`. |
| `compression`     | No       | Compression algorithm for the underlying Tantivy segments. Options: `none`, `zstd`, `lz4`. (Default: `zstd`).                                                                                                                                                                                                                                                                           |

---

## Schema Modes Explained

In edge environments, logging formats can change rapidly. The schema `mode` allows you to control ingestion strictness to protect your device's storage and memory.

- **`strict`**: If an incoming JSON document contains a field that is _not_ defined in the `fields` section, the entire document is rejected with a `400 Bad Request`.
- **`drop_unmapped`**: If a document contains undefined fields, the document is ingested, but the unknown fields are silently discarded. This guarantees only explicitly mapped data takes up disk space.
- **`dynamic`** (Default): All incoming fields are accepted. Unknown fields are preserved in `_source` but only explicitly defined fields are indexed for search. Recommended for development; use `drop_unmapped` or `strict` in production to prevent schema bloat.

---

## Field Types

When defining `fields`, you must specify how the underlying search engine should store the data.

| Type       | Description                                                                                                    |
| :--------- | :------------------------------------------------------------------------------------------------------------- |
| `text`     | Full-text searchable string. Text is tokenized, meaning you can search for individual words within a sentence. |
| `keyword`  | Exact-match string. Ideal for IDs, tags, log levels (`INFO`, `ERROR`), or hostnames.                           |
| `datetime` | Timestamp data. Required for the field specified in `timestamp_field` if you want time-based aggregations.     |
| `integer`  | Signed 64-bit integer.                                                                                         |
| `float`    | 64-bit floating point number.                                                                                  |
| `boolean`  | True/false values.                                                                                             |
| `bytes`    | Binary data.                                                                                                   |

---

## Field Properties

Every field can be fine-tuned to balance search speed, aggregation capabilities, and disk footprint.

| Property   | Default | Description                                                                                                                                                                                                                                                                  |
| :--------- | :------ | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `type`     | **Yes** | The data type (from the table above).                                                                                                                                                                                                                                        |
| `indexed`  | `false` | If `true`, the field is added to the inverted index, allowing it to be filtered or searched in the `query` clause of a request.                                                                                                                                              |
| `fast`     | `false` | If `true`, the field is stored in a columnar format (FastField). **Required for sorting or aggregations (stats, terms, date_histogram).** Note: Setting `fast: true` increases RAM usage during queries.                                                                     |
| `stored`   | `false` | If `true`, the individual field value is saved to disk so it can be retrieved separately. **Note:** Edgewit automatically stores the original JSON payload as `_source`. You rarely need `stored: true` unless you want to bypass JSON parsing for extreme read performance. |
| `optional` | `false` | If `false`, the system enforces that this field exists and is not `null` in every document. If `true`, the field can be missing or `null` without causing an ingestion error.                                                                                                |

---

## Storage Architecture

Each logical index corresponds to exactly one Tantivy index directory on disk:

```text
data/
└── indexes/
    ├── logs.index.yaml       ← schema definition file
    ├── logs/                 ← Tantivy index for the 'logs' index
    │   ├── meta.json
    │   ├── *.segment
    │   └── ...
    ├── sensors.index.yaml
    └── sensors/
        └── ...
```

A single Tantivy [`IndexWriter`](https://docs.rs/tantivy/latest/tantivy/struct.IndexWriter.html) is maintained per index. Documents from all time ranges land in the same index, making full-text search and aggregations work uniformly without any fan-out.

### Time-Based Retention

When an index has a `retention` value set, the background retention worker runs every 5 minutes and:

1. Computes a **cutoff timestamp**: `now() - retention_duration`
2. Issues a **Tantivy `delete_query`** with a date range `[Unbounded, cutoff)` on the `timestamp_field` column
3. The staged deletions are **flushed to disk** on the indexer's next regular commit (within `EDGEWIT_COMMIT_INTERVAL_SECS`, default 5 s)
4. Deleted documents are **physically reclaimed** when the compaction worker next merges segments

This means document removal is asynchronous but bounded: data older than `retention` will be gone within roughly `EDGEWIT_COMMIT_INTERVAL_SECS + EDGEWIT_COMPACTION_INTERVAL_SECS` of the retention worker's cycle.

**Requirements for retention to work:**

| Requirement                             | Why                                                                                    |
| :-------------------------------------- | :------------------------------------------------------------------------------------- |
| `timestamp_field` declared in `fields`  | The field must exist in the Tantivy schema                                             |
| `type: datetime` on the timestamp field | Enables the date range query                                                           |
| `fast: true` on the timestamp field     | Uses the columnar fast field for an O(matching docs) scan instead of a full index scan |

If the timestamp field is missing or is not `type: datetime`, the purge is skipped with an error log. If `fast: true` is absent, the purge still runs but logs a warning and performs a slower full scan.

### Disk Size Limits

Independent of per-index retention, global disk limits guard against storage exhaustion:

- **`EDGEWIT_MAX_INDEX_BYTES`** (default `1GB`): Maximum allowed size for all Tantivy index directories combined. Exceeding this threshold logs a warning.
- **`EDGEWIT_MAX_WAL_BYTES`** (default `512MB`): Maximum allowed size for uncommitted WAL files. When exceeded, old WAL files are pruned (emergency circuit-breaker to prevent SD card exhaustion).

---

## Startup Recovery (Fail-Fast)

Because Edgewit is designed for edge environments where power loss is common, it implements a highly deterministic startup recovery process:

1. **Disk as Source of Truth**: On startup, Edgewit reads all `.index.yaml` files from disk. If any file is malformed, invalid, or violates schema rules, Edgewit **fails to start** and logs a clear error. This prevents the system from running in a partially degraded state.
2. **Metadata Verification**: It checks internal Tantivy segment metadata to verify consistency and locate the exact Write-Ahead Log (WAL) offset where it last successfully committed data.
3. **Synchronous WAL Replay**: Before opening the API to accept new ingestion, Edgewit synchronously replays any remaining uncommitted events from the Write-Ahead Log into the index.

---

## Dynamic Index Management (API)

While you can manage indexes via configuration management (e.g., Ansible, K3s) by writing `.index.yaml` files to `data/indexes/` before Edgewit starts, you can also manage them at runtime via the REST API.

If an index is created or updated via the API, Edgewit immediately applies the changes in memory and persists the `.index.yaml` file to disk so that the state survives a reboot.

### Create or Update an Index

`PUT /indexes/<index-name>`

```bash
curl -X PUT http://localhost:9200/indexes/sensors \
  -H "Content-Type: application/json" \
  -d '{
    "name": "sensors",
    "timestamp_field": "measured_at",
    "mode": "drop_unmapped",
    "fields": {
      "measured_at": { "type": "datetime", "fast": true },
      "temperature":  { "type": "float",    "fast": true },
      "sensor_id":    { "type": "keyword",  "indexed": true }
    }
  }'
```

### Get Index Definition

`GET /indexes/<index-name>`

```bash
curl http://localhost:9200/indexes/sensors
```

### Delete an Index

`DELETE /indexes/<index-name>`

_Warning: This permanently deletes the index definition and immediately wipes all underlying search data._

```bash
curl -X DELETE http://localhost:9200/indexes/sensors
```

### List All Indexes

`GET /indexes`

Returns a JSON array of all registered `IndexDefinition` schemas currently active on the device.

### Check Index Health & Stats

`GET /_cat/indexes`

Returns a fast, OpenSearch-compatible tabular JSON array showing the active health, document count, and storage size of all initialized indexes.

### Security Configuration

In secure or regulated edge environments, you may want to lock the device to read-only schema management by setting:

`EDGEWIT_API_INDEX_MANAGEMENT_ENABLED=false`

When disabled, `PUT`/`DELETE` requests to `/indexes` return `403 Forbidden`.
