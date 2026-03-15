---
layout: default
title: Index Schema & Definition
---

# Edgewit Index Schema & Definition

Edgewit is designed to provide highly deterministic, fast, and stable search capabilities in edge environments. To achieve this, Edgewit relies on explicit **Index Definitions** (schemas).

Unlike heavy cloud-native search engines that rely on complex distributed cluster state, Edgewit stores index definitions locally as simple, human-readable YAML files. These files act as the absolute source of truth upon startup, ensuring that if an edge device loses power or connectivity, it recovers instantly and reliably.

---

## The Index Definition File

An index definition maps your JSON documents to Tantivy's underlying high-performance search schema. It dictates what fields are searchable, what fields can be aggregated, and how data is partitioned and retained.

_(Note on terminology: In Edgewit, we use **"indexes"** rather than "indices" as the plural of index. This aligns with modern computer science standards and, by explicitly using an `/indexes/` path prefix in the API, we avoid the root-level routing conflicts that system like OpenSearch often experience)._

### File Location and Naming

Index definitions are stored in the `/indexes/` subdirectory of your configured `EDGEWIT_DATA_DIR` (default: `./data/indexes/`).

Files must be named using the pattern: `<index-name>.index.yaml`. For example, the definition for the `logs` index must be named `logs.index.yaml`.

---

## Basic Example

Here is a complete example of a typical `logs.index.yaml` file:

```yaml
name: logs
description: "Application log events from edge services"

# The explicit field used for time partitioning and retention routing
timestamp_field: timestamp

# Enforcement mode: 'strict', 'drop_unmapped', or 'dynamic'
mode: drop_unmapped

# Time partitioning strategy: 'none', 'daily', 'hourly', 'monthly'
partition: daily

# Data retention policy (e.g., 7 days, 1 month, 12 hours)
retention: 7d

# Segment compression algorithm: 'none', 'zstd', 'lz4'
compression: zstd

fields:
  timestamp:
    type: datetime
    indexed: true
    fast: true

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

| Property          | Required  | Description                                                                                                                                                                                                                     |
| :---------------- | :-------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `name`            | **Yes**   | The unique name of the index. This must match the API endpoint (e.g., `PUT /indexes/logs`) and the filename prefix.                                                                                                             |
| `description`     | No        | A human-readable description of the index's purpose.                                                                                                                                                                            |
| `timestamp_field` | **Yes**\* | The explicit JSON field used to route documents to time partitions and calculate data expiration. (\*Required if `partition` is not `none`. Default is `timestamp`).                                                            |
| `mode`            | No        | Defines how Edgewit handles JSON fields that are not explicitly defined in the schema. Options: `strict`, `drop_unmapped`, `dynamic`. (Default: `dynamic`).                                                                     |
| `partition`       | No        | Strategy for partitioning data into separate physical directories over time. Crucial for efficient retention and fast time-based queries. Options: `none`, `daily`, `hourly`, `monthly`. (Default: `none`).                     |
| `retention`       | No        | Automatically deletes entire partition directories older than this duration based on the partition time. Format: Number followed by unit (`s`, `m`, `h`, `d`, `w`, `M`, `Y`). Example: `30d`. _Requires `partition` to be set._ |
| `compression`     | No        | Compression algorithm for the underlying Tantivy segments. Options: `none`, `zstd`, `lz4`. (Default: `zstd`).                                                                                                                   |

---

## Schema Modes Explained

In edge environments, logging formats can change rapidly. The schema `mode` allows you to control ingestion strictness to protect your device's storage and memory.

- **`strict`**: If an incoming JSON document contains a field that is _not_ defined in the `fields` section, the entire document is rejected with a `400 Bad Request`.
- **`drop_unmapped`**: If a document contains undefined fields, the document is ingested, but the unknown fields are silently discarded. This guarantees only explicitly mapped data takes up disk space.
- **`dynamic`** (Default): Edgewit will attempt to automatically infer the type of any new, undefined fields and add them to the index. While convenient for development, it is generally recommended to use `drop_unmapped` or `strict` in production to prevent schema bloat.

---

## Field Types

When defining `fields`, you must specify how the underlying search engine should store the data.

| Type       | Description                                                                                                    |
| :--------- | :------------------------------------------------------------------------------------------------------------- |
| `text`     | Full-text searchable string. Text is tokenized, meaning you can search for individual words within a sentence. |
| `keyword`  | Exact-match string. Ideal for IDs, tags, log levels (`INFO`, `ERROR`), or hostnames.                           |
| `datetime` | Timestamp data. Required for the field specified in `timestamp_field`.                                         |
| `integer`  | Signed 64-bit integer.                                                                                         |
| `float`    | 64-bit floating point number.                                                                                  |
| `boolean`  | True/false values.                                                                                             |
| `bytes`    | Binary data.                                                                                                   |

---

## Field Properties

Every field can be fine-tuned to balance search speed, aggregation capabilities, and disk footprint.

| Property   | Default | Description                                                                                                                                                                                                                                                                         |
| :--------- | :------ | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `type`     | **Yes** | The data type (from the table above).                                                                                                                                                                                                                                               |
| `indexed`  | `false` | If `true`, the field is added to the inverted index, allowing it to be filtered or searched in the `query` clause of a request.                                                                                                                                                     |
| `fast`     | `false` | If `true`, the field is stored in a columnar format (FastField). **This is required if you want to sort by the field or run aggregations (stats, terms) on it.** Note: Setting `fast: true` increases RAM usage during queries.                                                     |
| `stored`   | `false` | If `true`, the individual field value is saved to disk so it can be retrieved separately. **Note:** Edgewit automatically stores the original JSON payload as `_source`. You rarely need to set `stored: true` unless you want to bypass JSON parsing for extreme read performance. |
| `optional` | `false` | If `false`, the system will enforce that this field exists in every document. (Only applies if `mode` is `strict`).                                                                                                                                                                 |

---

## Partitioning & Retention Architecture

Edgewit physically separates data on the disk based on the `partition` strategy. This allows Edgewit to enforce retention policies with **zero I/O overhead**.

Instead of opening massive search indexes and executing expensive "delete by query" operations to prune old logs (which thrashes SD cards), Edgewit's background worker simply identifies expired partition directories and deletes them entirely from the filesystem.

**Example Structure:**
If `partition: daily` and `retention: 7d` are set, the underlying disk will look like this:

```text
data/
└── indexes/
    └── logs/
        └── segments/
            ├── 2023-10-01/  <-- If today is 10-09, this folder is deleted instantly
            ├── 2023-10-02/
            ├── 2023-10-03/
            └── ...
```

_Note: Retention policies (`retention`) are completely ignored if `partition: none` is set, because Edgewit cannot safely delete a monolithic index without scanning it._

---

## Startup Recovery (Fail-Fast)

Because Edgewit is designed for edge environments where power loss is common, it implements a highly deterministic startup recovery process:

1. **Disk as Source of Truth**: On startup, Edgewit reads all `.index.yaml` files from the disk. If any file is malformed, invalid, or violates schema rules, Edgewit will **fail to start** and log a clear error. This prevents the system from running in a partially degraded or unpredictable state.
2. **Metadata Verification**: It checks internal Tantivy segment metadata to verify consistency and locate the exact Write-Ahead Log (WAL) offset where it last successfully committed data.
3. **Synchronous WAL Replay**: Before opening the API to accept new ingestion, Edgewit synchronously replays any remaining uncommitted events from the Write-Ahead Log into the index.
4. **Partition State Recovery**: The background compaction and retention workers immediately scan partition directories on disk to rebuild their state, ensuring storage limits are continuously enforced.

---

## Dynamic Index Management (API)

While you can manage indexes via a configuration management tool (like Ansible or K3s) by simply writing `.index.yaml` files to the `data/indexes/` folder before Edgewit starts, you can also manage them dynamically at runtime via the REST API.

If an index is created or updated via the API, Edgewit will immediately apply the changes in memory and persist the `.index.yaml` file to disk so that the state survives a reboot.

### Create or Update an Index

`PUT /indexes/<index-name>`

```bash
curl -X PUT http://localhost:9200/indexes/sensors \
  -H "Content-Type: application/json" \
  -d '{
    "name": "sensors",
    "timestamp_field": "measured_at",
    "mode": "drop_unmapped",
    "partition": "daily",
    "retention": "30d",
    "fields": {
      "measured_at": { "type": "datetime", "fast": true },
      "temperature": { "type": "float", "fast": true },
      "sensor_id":   { "type": "keyword", "indexed": true }
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

_Warning: This will permanently delete the index definition and immediately wipe all underlying search segments and data._

```bash
curl -X DELETE http://localhost:9200/indexes/sensors
```

### List All Indexes

`GET /indexes`

Returns a complete JSON array of all registered `IndexDefinition` schemas currently active on the device.

### Check Index Health & Stats

`GET /_cat/indexes`

Returns a fast, OpenSearch-compatible tabular JSON array showing the active health, document count, and storage size of all initialized indexes.

### Security Configuration

In highly secure or regulated edge environments, you may not want external applications to dynamically create or destroy indexes via the API. You can strictly lock the device down to read-only schema management by setting the environment variable:

`EDGEWIT_API_INDEX_MANAGEMENT_ENABLED=false`

When disabled, Edgewit will only load schemas from the `.index.yaml` files present on the disk at startup, and `PUT`/`DELETE` requests to `/indexes` will return `403 Forbidden`.
