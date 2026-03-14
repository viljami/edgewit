---
layout: default
title: Security Model
---

<div class="card" markdown="1">
## Overview

Edgewit is designed primarily for trusted edge environments such as Raspberry Pi devices, embedded systems, and internal infrastructure networks. The system prioritizes simplicity, predictable performance, and minimal resource consumption.

Security mechanisms therefore focus on infrastructure-level protection and lightweight optional controls rather than full internal user management.
</div>

<div class="card" markdown="1">
## Default Security Model (Trusted Network)

Default deployments assume Edgewit runs inside a trusted environment such as a private network, internal cluster network, or a single-device observability node. By default, Edgewit expects no authentication, keeping binary size small, runtime overhead low, and deployment straightforward.

*   Service binds to internal interfaces.
*   No authentication required by default.
*   Intended for internal services or developers.
</div>

<div class="card" markdown="1">
## API Key Authentication

For deployments that require lightweight access protection, Edgewit provides a simple API key mechanism via environment variables. This creates an extremely small implementation footprint with no persistent state required.

**Configuration Example:**

```bash
EDGEWIT_API_KEY=abc123
```

**Request Example:**

```http
Authorization: Bearer abc123
```

When configured, all HTTP endpoints—including ingestion and search—will require this valid API key. This is useful for multi-application internal networks and lightweight edge protection.
</div>

<div class="card" markdown="1">
## Infrastructure Authentication (Recommended)

For deployments exposing Edgewit to less trusted networks or requiring stronger access controls, authentication should be implemented using external infrastructure. This model allows flexible authentication strategies while keeping Edgewit's binary small and highly performant.

Examples of infrastructure authentication:

*   Reverse Proxy Authentication (Nginx, Traefik, Caddy)
*   Service Mesh Identity
*   VPN Access Control
*   OAuth via Reverse Proxy
</div>

<div class="card" markdown="1">
## Security Non-Goals

To keep the system small, predictable, and easy to operate on constrained devices, Edgewit deliberately does **not** implement the following:

*   User accounts & Password databases
*   Role-based Access Control (RBAC)
*   OAuth providers or Identity federation
*   Session management
*   Authorization policy engines

These responsibilities are intentionally delegated to surrounding infrastructure components when needed.
</div>

<div class="card" markdown="1">
## Security Best Practices

Recommended guidelines for operating Edgewit securely:

1.  Run Edgewit inside trusted networks whenever possible.
2.  Use firewall rules (e.g., `ufw` or `iptables`) to restrict access.
3.  Enable API key authentication (`EDGEWIT_API_KEY`) for shared environments.
4.  Place a reverse proxy with proper authentication in front of any public-facing deployments.
5.  Avoid exposing ingestion endpoints (`/_bulk`) directly to the internet.
6.  Rotate API keys periodically in automated environments.
</div>

<div class="card" markdown="1">
## Summary

Edgewit's design goal is to remain simple, secure, and suitable for constrained edge environments. By relying on robust, infrastructure-first security models alongside optional, high-speed API keys, Edgewit balances the tight constraints of edge systems with necessary protection mechanisms.
</div>
