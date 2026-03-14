---
layout: post
title:  "First Release of Edgewit: Bringing Search to the Edge"
date:   2026-03-14 00:00:00 +0000
categories: release
---

Welcome to the very first release of Edgewit! 

We are incredibly excited to introduce our lightweight, Rust-based search and analytics engine, purpose-built from the ground up to thrive in edge environments and on constrained hardware.

## Why Edgewit?

The story of Edgewit starts with a homelab and a Raspberry Pi. Initially, the plan was to use [Quickwit](https://quickwit.io/) for local observability and log analytics. Quickwit is an absolutely fantastic engine for distributed search, but we quickly ran into a major roadblock when trying to deploy it on edge hardware. 

Specifically, there is a lingering `jemalloc` issue ([quickwit-oss/quickwit#4785](https://github.com/quickwit-oss/quickwit/issues/4785)) that prevents the public Quickwit container from running successfully on a Raspberry Pi. With no immediate progress or workarounds for that issue on the horizon, the need for a truly edge-native alternative became clear.

We needed something that:
1. **Actually runs on ARM/Raspberry Pi** out of the box without complex compilation flags or memory allocator crashes.
2. **Has a strict memory ceiling**, comfortably operating under 150MB of RAM.
3. **Is resilient to slow storage** (like the cheap SD cards commonly found in IoT and edge devices).

Thus, Edgewit was born. By combining the incredible performance of the `tantivy` search engine library with a custom, SD-card-optimized Write-Ahead Log (WAL), we've created a search engine that fits perfectly into the edge ecosystem.

## Looking to the Future

This first release proves the core concept: durable ingestion, dynamic schema mapping, and OpenSearch-compatible querying running efficiently on constrained hardware. But we are just getting started. 

Here is a brief outline of what we are planning for the future of Edgewit:
* **Expanded OpenSearch Compatibility:** We will continue building out the subset of OpenSearch APIs to ensure drop-in compatibility with popular log shippers like Vector, Fluent Bit, and Logstash.
* **Advanced Aggregations:** Enhancing our query parser to support more complex metrics and bucket aggregations directly at the edge.
* **Cluster & Replication (Lightweight):** Exploring peer-to-peer replication options for highly available edge clusters without the overhead of heavy coordination systems like ZooKeeper or etcd.
* **Plug-and-Play Observability:** Providing official Grafana dashboard templates designed specifically for Edgewit's telemetry.

We invite you to pull the latest container from our Docker Hub (`viljamip/edgewit:latest`), spin it up on your Raspberry Pi, and let us know what you think! 

Happy logging!
