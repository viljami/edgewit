PROJECT: edgewit

VISION
Edgewit is a lightweight, Rust-based search and analytics engine designed for edge environments. Inspired by Quickwit, it implements a focused subset of the OpenSearch API to provide powerful full-text search and aggregations on constrained hardware such as Raspberry Pi and embedded systems. Edgewit enables local observability, log analytics, and event exploration without dependence on cloud infrastructure. It prioritizes simplicity, deterministic performance, low memory usage, and reliable operation on ARM devices while remaining compatible with common OpenSearch query patterns.

MISSION
Deliver a fast, minimal, embeddable search engine capable of ingesting structured events and logs, indexing them efficiently, and providing powerful aggregation queries locally. Edgewit should run comfortably inside containers on Raspberry Pi-class devices and integrate easily with existing observability tooling that expects OpenSearch-like APIs.

CORE PRINCIPLES

- edge-first architecture
- minimal operational complexity
- deterministic resource usage
- compatibility with OpenSearch query patterns
- Rust safety and performance
- small binary footprint
- resilient on unreliable hardware
- simple deployment (single container)

TARGET ENVIRONMENT

- Raspberry Pi 4/5
- ARM64 and ARMv7
- container deployment
- 512MB–8GB RAM
- local SSD or SD storage
- intermittent connectivity environments

PRIMARY USE CASES

- local log search on edge devices
- IoT gateway observability
- robotics telemetry inspection
- factory equipment diagnostics
- offline analytics
- distributed edge debugging
- homelab monitoring

NON-GOALS

- large distributed clusters
- cloud-scale indexing
- full OpenSearch feature parity
- multi-tenant SaaS use

CORE CAPABILITIES

- structured event ingestion
- full text search
- time filtering
- aggregations
- lightweight indexing
- local storage
- retention management
- container-first deployment

ARCHITECTURE COMPONENTS

- HTTP ingest API
- WAL event buffer
- batch indexer
- Tantivy index segments
- query engine
- aggregation engine
- segment compactor
- retention manager
- metrics endpoint

INDEX MODEL

- append-only ingestion
- immutable segments
- background compaction
- timestamp optimized search
- small schema footprint

API SURFACE (OPENSEARCH SUBSET)

- POST /index/\_doc
- POST /\_bulk
- GET /\_search
- GET /\_health
- GET /\_stats

SUPPORTED QUERY FEATURES

- match
- term
- range
- boolean queries
- time filtering
- sorting
- pagination

AGGREGATIONS (INITIAL)

- terms aggregation
- date histogram
- count
- min/max
- average

DEPLOYMENT MODEL

- single binary
- container image
- persistent volume for segments
- configurable memory limits
- no external dependencies

PERFORMANCE TARGETS

- <150MB resident memory
- <50MB indexing buffer
- stable operation on 2GB Pi
- sub-second search on millions of events
- sustained ingestion ~5k events/sec on Pi4

PROJECT MILESTONES

M0 PROJECT FOUNDATION [DONE]
goal:
establish minimal runnable system and repository structure

deliverables:

- repository initialization
- crate layout
- config system
- logging
- container build
- CI pipeline (github actions)
- ARM cross compilation
- local storage layout

success criteria:
binary runs on raspberry pi and exposes health endpoint

M1 INGESTION PIPELINE [DONE]
goal:
accept and persist events reliably

deliverables:

- HTTP ingestion endpoint
- JSON event schema
- bulk ingestion endpoint
- WAL implementation
- batching mechanism
- ingestion metrics

success criteria:
events safely persisted and recoverable after crash

M2 INDEXING ENGINE [DONE]
goal:
convert ingested events into searchable segments

deliverables:

- Tantivy schema builder
- index writer
- batching and flush strategy
- segment creation
- WAL replay on startup

success criteria:
events become searchable after ingestion

M3 SEARCH ENGINE
goal:
support OpenSearch-like search queries

deliverables:

- query parser
- boolean queries
- full text search
- range queries
- sorting
- pagination
- search endpoint

success criteria:
sub-second search over millions of documents on Pi

M4 AGGREGATION ENGINE
goal:
support analytical queries locally

deliverables:

- terms aggregation
- histogram aggregation
- metric aggregations
- aggregation planner
- efficient field access

success criteria:
aggregations work on indexed data with predictable latency

M5 SEGMENT MANAGEMENT
goal:
maintain healthy index structure

deliverables:

- segment compaction
- merge policy
- retention policy
- disk usage limits
- background compaction worker

success criteria:
index remains performant over long-running ingestion

M6 EDGE OPTIMIZATION
goal:
optimize for constrained hardware

deliverables:

- memory budgeting
- configurable search threads
- lightweight caching
- ARM performance tuning
- disk write reduction

success criteria:
stable operation on Raspberry Pi with limited memory

M7 OPENSEARCH COMPATIBILITY
goal:
make integration simple for existing tools

deliverables:

- compatible search request structure
- compatible aggregation response format
- OpenSearch style index naming
- minimal compatibility layer

success criteria:
basic OpenSearch clients can query edgewit

M8 OBSERVABILITY
goal:
make system introspectable and debuggable

deliverables:

- metrics endpoint
- ingestion stats
- query latency stats
- segment statistics
- Prometheus compatibility

success criteria:
operators can understand performance and health

M9 DISTRIBUTED EDGE (FUTURE)
goal:
enable federation across multiple devices

deliverables:

- node discovery
- query fan-out
- partial result merging
- cluster health

success criteria:
queries across multiple edge nodes

LONG TERM VISION
Edgewit becomes the standard lightweight search and analytics engine for edge infrastructure. It enables developers to run powerful search and aggregation locally on devices without requiring centralized log systems. Over time, it expands toward a federated edge observability layer where devices can search locally first and collaborate across networks when needed.
