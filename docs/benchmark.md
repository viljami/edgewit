---
layout: default
title: Benchmarks
---

<article class="card" markdown="1">
## David vs. Goliath at the Edge: Benchmarking a Rust OpenSearch Alternative

Observability at the edge is broken. If you've ever tried running a JVM-based OpenSearch or Elasticsearch node on a Raspberry Pi or a $5/month VPS alongside your actual application, you know it's practically impossible. The JVM demands hundreds of megabytes just to wake up, and indexing log data quickly leads to CPU throttling and Out-Of-Memory (OOM) crashes.

Enter **Edgewit**: a lightweight, OpenSearch-compatible log search engine written in Rust.

### The Architecture: How Edgewit works

> **Note:** Edgewit does not support cluster or distributed configurations. It is intentionally single-node only—there is no cluster configuration option, and this is unlikely to change in the future. This keeps the system simple, predictable, and perfectly suited for edge and embedded deployments.

Edgewit is designed specifically for constrained environments, utilizing:

- **Axum** for low-overhead, zero-cost HTTP routing.
- **An Adaptive WAL (Write-Ahead Log)** for crash-safe, high-speed ingestion. It batches writes dynamically, which is crucial for preserving the lifespan of slow SD cards on edge devices.
- **Tantivy** for memory-mapped, high-performance Lucene-style indexing without the JVM bloat.

### The Setup

To prove Edgewit's efficiency, we simulated an edge/micro-cloud hardware profile (1 vCPU limit).

- **Edgewit** was constrained to **256MB** of total container RAM.
- **OpenSearch** was given a more generous **768MB** limit with a 512MB JVM heap (anything lower and it refuses to start).
- **Dataset:** We used a 100,000 document subset of the OpenSearch Rally `http_logs` dataset, sent in 5,000-document NDJSON chunks via the `/_bulk` API with 10 concurrent connections.

### The Results: The Showdown

We hypothesized that Edgewit would outperform OpenSearch, but the results were a slaughter.

#### 1. Memory Usage

- **OpenSearch:** Thrashing. It consumed its entire heap, spending up to 18 seconds per Garbage Collection cycle just trying to stay alive.
- **Edgewit:** Peaked at an astonishing **~25 MB** of RAM usage under maximum load (less than 10% of its already tiny limit).

#### 2. Bulk Ingestion Speed

- **OpenSearch:** **6,800 docs/sec**. The node struggled to process concurrent bulk requests.
- **Edgewit:** Sustained **86,200 docs/sec**. The adaptive WAL absorbed the concurrent requests effortlessly, bypassing standard IO bottlenecks.

#### 3. Search & Aggregation Latency

Edgewit dominated across the board:

| Metric                   | Edgewit         | OpenSearch     | Advantage    |
| :----------------------- | :-------------- | :------------- | :----------- |
| **Ingestion Throughput** | 86,200 docs/sec | 6,800 docs/sec | 12.7x faster |
| **Match All**            | 11,963 req/sec  | 379 req/sec    | 31x faster   |
| **Term Search**          | 13,684 req/sec  | 335 req/sec    | 41x faster   |
| **Aggregations**         | 15,912 req/sec  | 730 req/sec    | 21.8x faster |

### Conclusion

Rust makes edge-native search not just possible, but incredibly performant. Edgewit achieved over 85k inserts per second using a fraction of the memory that caused OpenSearch to struggle.

If you're building for IoT, running micro-clouds, or just want to lower your observability AWS bill, check out the GitHub repository and run it on your own edge devices today!

</article>
