# Edgewit Benchmark & Launch Plan

## Objective

Quantify and prove Edgewit's performance and efficiency advantages over OpenSearch/Elasticsearch in low-resource and edge environments (IoT, Raspberry Pi, small VPS). The final output will be a comprehensive set of reproducible benchmarks and a polished blog post.

---

## Phase 1: Environment & Dataset Preparation

### 1. Target Hardware

To prove the "edge" capabilities, we will run the benchmarks on restricted hardware where JVM-based systems typically struggle:

- **Hardware Profile A (The Edge):** Raspberry Pi 4 (2GB or 4GB RAM, Quad-core ARM) running off an SD card or USB SSD.
- **Hardware Profile B (The Micro-Cloud):** A standard $5/month VPS (1 vCPU, 1GB RAM).

### 2. Standardized Datasets

Instead of synthetic `hello_world` logs, we will use industry-standard datasets from OpenSearch Rally:

- **HTTP Logs (`http_logs`):** 30GB+ of web server logs. Great for testing raw text indexing and ingestion throughput.
- **NYC Taxis (`nyc_taxis`):** Structured data heavy on numerics and timestamps. Perfect for testing complex aggregations (`date_histogram`, `sum`, `avg`).

### 3. Baseline Setup

- **Edgewit:** Compiled in `--release` mode.
- **OpenSearch:** Running via standard Docker. We will attempt to constrain the JVM heap (`-Xms512m -Xmx512m`) to match the hardware profile.

---

## Phase 2: Ingestion Benchmarks (`/_bulk`)

**Tooling:** Use `oha` (Rust-based HTTP load generator) or `wrk` to hammer the `/_bulk` endpoints.

**Test Execution:**

1.  Push 1,000 to 5,000 document NDJSON payloads sequentially and concurrently.
2.  **Metrics to capture:**
    - Peak indexing throughput (Documents / second).
    - Bandwidth (MB / second).
    - System resource utilization (CPU % and peak RAM usage via `htop` or `docker stats`).

**Hypothesis to Prove:**
Edgewit's adaptive WAL batching will allow it to sustain 10k–50k docs/sec utilizing less than 100MB of RAM, bypassing standard IO bottlenecks. OpenSearch will likely encounter JVM Garbage Collection thrashing, throttling, or outright Out-Of-Memory (OOM) crashes under the same constraints.

**Status & Results (Completed):**

- **Edgewit:** Successfully ingested the payload chunk (5,000 docs/request, 10 concurrent connections). Memory usage peaked at just **~25 MB**. Throughput sustained at **~275,000 to 320,000 documents/sec**.
- **OpenSearch:** Completely failed to ingest under the 768MB container / 512MB heap limit. Throughput was **0 docs/sec** as the node entered a state of continuous JVM Garbage Collection thrashing (spending 18+ seconds per GC cycle), locking up the server.

---

## Phase 3: Search & Aggregation Latency (`/_search`)

**Tooling:** Run `oha` targeting specific search queries against the fully populated indexes.

**Test Execution:**
Run 10,000 queries at varying concurrency levels for:

1.  `match_all` (Baseline network/parsing overhead).
2.  `term` search (Full-text lookup).
3.  Complex aggregations (`date_histogram` by day/hour + `sum` metric).

**Metrics to capture:**

- Latency Percentiles: p50, p90, and p99.

**Hypothesis to Prove:**
Because Edgewit avoids JVM networking bloat and relies on Tantivy's memory-mapped (mmap) segments, query latencies will sit in the **microsecond** range (<1ms), whereas OpenSearch will baseline in the **millisecond** range (2-20ms+).

**Status & Results (Completed):**

OpenSearch required its heap limit to be bumped to 1GB just to stay alive during these tests.

- **Match All (Baseline Network Overhead):**
  - **Edgewit:** 3,402 req/sec (Avg Latency: 2.95ms)
  - **OpenSearch:** 255 req/sec (Avg Latency: 43.62ms)
  - _Edgewit is ~13x faster._
- **Term Search (Full-text lookup):**
  - **Edgewit:** 6,558 req/sec (Avg Latency: 2.40ms)
  - **OpenSearch:** 63 req/sec (Avg Latency: 159.18ms)
  - _Edgewit is ~104x faster._
- **Aggregations (Terms Aggregation):**
  - **Edgewit:** 1,730 req/sec (Avg Latency: 6.08ms)
  - **OpenSearch:** 108 req/sec (Avg Latency: 95.63ms)
  - _Edgewit is ~16x faster._

---

## Phase 4: The Blog Post (Draft)

**Title:** _David vs. Goliath at the Edge: Benchmarking a Rust OpenSearch Alternative_

### The Hook

Observability at the edge is broken. If you've ever tried running a JVM-based OpenSearch or Elasticsearch node on a Raspberry Pi or a $5/month VPS alongside your actual application, you know it's practically impossible. The JVM demands hundreds of megabytes just to wake up, and indexing log data quickly leads to CPU throttling and Out-Of-Memory (OOM) crashes.

Enter **Edgewit**: a lightweight, OpenSearch-compatible log search engine written in Rust.

### The Architecture: How Edgewit works

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

**1. Memory Usage**

- **OpenSearch:** Thrashing. It consumed its entire heap, spending up to 18 seconds per Garbage Collection cycle just trying to stay alive.
- **Edgewit:** Peaked at an astonishing **~25 MB** of RAM usage under maximum load (less than 10% of its already tiny limit).

**2. Bulk Ingestion Speed**

- **OpenSearch:** **0 docs/sec**. The node completely locked up due to GC thrashing and failed to process the concurrent bulk requests.
- **Edgewit:** Sustained a massive **275,000 to 320,000 docs/sec**. The adaptive WAL absorbed the concurrent requests effortlessly, bypassing standard IO bottlenecks.

**3. Search & Aggregation Latency**

Even after we graciously bumped OpenSearch's RAM allocation to 1.5GB (with a 1GB heap) just so it could serve queries without crashing, Edgewit dominated across the board:

- **Match All (Baseline Network Overhead):** Edgewit served **3,400+ req/sec** (2.95ms latency) vs OpenSearch's **255 req/sec** (43ms latency). That's a 13x improvement just in routing and protocol overhead.
- **Term Search (Full-Text):** Edgewit served **6,500+ req/sec** (2.40ms latency) vs OpenSearch's measly **63 req/sec** (159ms latency). Edgewit is over **100x faster**.
- **Aggregations:** Edgewit served **1,730 req/sec** (6.08ms latency) vs OpenSearch's **108 req/sec** (95ms latency). A 16x improvement.

### Conclusion

Rust makes edge-native search not just possible, but incredibly performant. Edgewit achieved over 250k inserts per second using a fraction of the memory that caused OpenSearch to crash entirely.

If you're building for IoT, running micro-clouds, or just want to lower your observability AWS bill, check out the GitHub repository and run it on your own edge devices today!
