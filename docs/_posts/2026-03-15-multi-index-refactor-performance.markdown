---
layout: post
title: "Performance Impact of the Multi-Index Refactor"
date: 2026-03-15 00:00:00 +0000
categories: engineering performance
---

We've just completed a major architectural overhaul of Edgewit's core storage engine! Today, we transitioned from a single monolithic Tantivy index to physically separated indexes per-index and per-partition.

This change brings significant architectural improvements, especially for multi-tenant edge environments, but it also comes with some interesting performance trade-offs.

## The Motivation

Previously, all incoming documents were dumped into a single generic index directory, utilizing a catch-all schema. While this was simple and incredibly fast for raw ingestion, it made enforcing strict schemas, partitioning data by time, and querying specific data domains highly inefficient.

By routing documents into physically separated indexes based on their definition (`logs`, `metrics`, etc.) and their time partition (`2026-03-15`), we can now achieve:

1. **Zero-Overhead Retention:** We can instantly delete expired data by dropping a partition's directory instead of issuing expensive "delete by query" operations.
2. **Strict Schema Enforcement:** Indexes can now adhere accurately to their individual YAML definitions.

## The Benchmark Results

To measure the impact of this new routing logic, we ran our standard Criterion benchmark suite before and after the refactor. Here are the results:

### Search Performance: Massive Wins! 🚀

Because our data is now cleanly partitioned and queries only target specific indexes instead of scanning the entire monolithic blob, search latency has plummeted!

- **Match All Queries (`search/match_all`):**
  - **Before:** ~145 µs
  - **After:** ~36 µs
  - **Improvement:** **~75% faster!**
- **Aggregations (`search/aggregations`):**
  - **Before:** ~175 µs
  - **After:** ~52 µs
  - **Improvement:** **~70% faster!**

This scatter-gather single-index search architecture means dashboards and analytic queries will load snappier than ever on constrained edge hardware.

### Ingestion Performance: The Expected Trade-off 📉

Routing documents across multiple physical index writers and dynamically creating partition directories on the fly comes with overhead.

- **Bulk Ingestion (1000 docs):**
  - **Before:** ~20 ms
  - **After:** ~50 ms
  - **Regression:** **~150% slower.**

While a 150% regression sounds steep, 50ms to durably ingest, route, and map 1000 documents across an edge WAL and multi-writer pool is still roughly **20,000 documents per second** on a single machine. For the vast majority of IoT and edge-logging use cases, this throughput remains more than sufficient, and the long-term benefits of partitioned storage are well worth the cost.

## What's Next?

It is also worth noting that **these benchmark results are entirely without optimization.** We've prioritized correctness and architectural soundness over raw speed for this initial refactor. Once we have accumulated some real-world data and usage patterns, we will come back to optimize these hot paths!

We will be looking into memory-pooling optimizations for the new `IndexManager` to ensure that having dozens of active partitions doesn't exhaust RAM on smaller devices like the Raspberry Pi Zero. We're also exploring ways to parallelize the partition routing logic to win back some of that ingestion throughput!

Stay tuned for more updates, and as always, happy logging!
