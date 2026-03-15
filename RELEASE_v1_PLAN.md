EDGEWIT RELEASE AND INTEGRATION MILESTONES

Once ruuvi home project is ipdated and working with balena - prepare a edgewit post with title: Running Edgewit on Raspberry Pi with Balena

M13 CONTAINER MULTI-ARCH BUILD (COMPLETED)

goal
produce official container images that run on both developer machines and edge devices

tasks

- [x] configure docker buildx builder
- [x] enable multi architecture builds
- [x] define supported targets:
  linux/amd64
  linux/arm64
- [x] ensure static or minimal dependency runtime
- [x] verify image runs locally on both architectures
- [x] test container startup
- [x] test container with persistent volume

code steps
docker buildx create --use
docker buildx build \
 --platform linux/amd64,linux/arm64 \
 -t ghcr.io/<repo>/edgewit:latest \
 --push .

success criteria

- docker pull works on amd64 laptop
- docker pull works on raspberry pi
- same image tag works for both architectures

M14 GITHUB ACTIONS AUTOMATED CONTAINER BUILDS (COMPLETED)

goal
automatically build and publish container images on push and release

tasks

- [x] create GitHub Actions workflow
- [x] enable docker buildx in CI
- [x] authenticate to GitHub container registry
- [x] build multi-arch container
- [x] push version tags
- [x] push latest tag

code steps
name: container-build

on:
push:
branches: [main]
release:
types: [published]

jobs:
build:
runs-on: ubuntu-latest

    steps:
      - [x] uses: actions/checkout@v4

      - [x] uses: docker/setup-buildx-action@v3

      - [x] uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - [x] uses: docker/build-push-action@v5
        with:
          context: .
          push: true
          platforms: linux/amd64,linux/arm64
          tags: |
            ghcr.io/<repo>/edgewit:latest
            ghcr.io/<repo>/edgewit:${{ github.ref_name }}

success criteria

- container automatically built on push
- container automatically published on release

M15 CONTAINER OPTIMIZATION (COMPLETED)

goal
minimize container size and startup time for edge deployments

tasks

- [x] implement multi-stage docker build
- [x] strip binary
- [x] use minimal runtime base image
- [x] remove build dependencies
- [x] measure image size
- [x] benchmark startup time

code steps
FROM rust:1.77 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/edgewit /usr/local/bin/edgewit
ENTRYPOINT ["edgewit"]

success criteria

- container image below 40MB
- startup time under 1 second

M16 VERSION AND BUILD METADATA (COMPLETED)

goal
expose runtime version information for debugging and deployments

tasks

- [x] add version endpoint
- [x] embed build metadata
- [x] expose commit hash
- [x] expose build timestamp

code steps
GET /version

example response
{
"version": "0.1.0",
"commit": "abc123",
"build": "2026-03-15"
}

success criteria

- version endpoint accessible
- CI injects commit metadata

M17 RUUV I HOME PROJECT INTEGRATION

goal
validate edgewit in real-world environment using ruuvi sensor data

tasks

- update ruuvi home ingestion pipeline
- send sensor data events to edgewit
- define sensor index schema
- verify ingestion throughput
- validate time-series queries
- implement example aggregations
- verify multi-index functionality
- benchmark query latency

example sensor event
{
"timestamp": "...",
"sensor_id": "...",
"temperature": 21.5,
"humidity": 48.2,
"pressure": 1012
}

success criteria

- ruuvi data successfully ingested
- queries produce correct results
- system stable on raspberry pi

M18 BALENA DEPLOYMENT

goal
enable device fleet deployment using balena

tasks

- create balena compatible container config
- configure persistent volume
- test deployment to raspberry pi
- verify automatic updates
- monitor runtime stability

deployment architecture

ruuvi sensors
↓
collector
↓
edgewit container
↓
query client or dashboard

success criteria

- device boots and starts edgewit
- ingestion pipeline works
- deployment reproducible across devices

M19 BENCHMARK VALIDATION ON RASPBERRY PI

goal
verify performance characteristics on actual hardware

tasks

- ingest synthetic events
- run aggregation queries
- measure ingestion throughput
- measure search latency
- monitor memory consumption
- document results

metrics to collect

- events per second ingestion
- query latency p50
- query latency p95
- memory usage
- disk usage

success criteria

- stable operation on Raspberry Pi
- acceptable latency for edge analytics

M20 DOCUMENTATION AND COMMUNITY RELEASE

goal
prepare project for public visibility and adoption

tasks

- finalize github pages documentation
- ensure quickstart instructions work
- publish container usage examples
- add architecture overview
- write benchmark blog post
- write security model documentation
- prepare community announcement posts

documentation sections

- quickstart
- container deployment
- index configuration
- API reference
- security model
- architecture overview

success criteria

- new user can run edgewit in under 2 minutes
- documentation complete

M21 COMMUNITY ANNOUNCEMENT

goal
introduce edgewit to developer community

tasks

- publish LinkedIn launch post
- submit project to Rust Weekly
- post to rust subreddit
- share in rust embedded community
- submit hacker news show post

success criteria

- initial community feedback collected
- project visibility established
