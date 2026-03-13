EDGEWIT SECURITY PLAN

SECURITY PHILOSOPHY
Edgewit is designed primarily for trusted edge environments such as Raspberry Pi devices, embedded systems, and internal infrastructure networks. The system prioritizes simplicity, predictable performance, and minimal resource consumption. Security mechanisms therefore focus on infrastructure-level protection and lightweight optional controls rather than full internal user management.

Edgewit intentionally avoids implementing internal user databases, role-based access control, or identity systems. Authentication and access control are expected to be handled by surrounding infrastructure when needed.

CORE SECURITY PRINCIPLES

- minimal attack surface
- no internal user management
- secure-by-deployment model
- infrastructure-first authentication
- optional lightweight access controls
- deterministic resource usage
- zero mandatory external dependencies

SECURITY LAYERS

LAYER 1 TRUSTED NETWORK MODEL (DEFAULT)
Default deployments assume Edgewit runs inside a trusted environment such as:

- private network
- internal cluster network
- single-device observability node
- development environments
- industrial networks
- home lab infrastructure

Default configuration behavior:

- service binds to localhost or internal interface
- no authentication required
- intended for internal services or developers

Typical deployment architecture:

collector or application
↓
edgewit (internal network)
↓
visualization or query client

Advantages:

- smallest binary size
- lowest runtime overhead
- simplest deployment
- ideal for Raspberry Pi devices
- zero configuration required

Recommended safeguards:

- avoid exposing Edgewit directly to the public internet
- run behind internal network boundaries
- restrict access via firewall rules if necessary

LAYER 2 OPTIONAL API KEY AUTHENTICATION
Edgewit provides a lightweight API key mechanism for deployments that require simple access protection.

Characteristics:

- single shared key
- HTTP header based authentication
- extremely small implementation footprint
- no database or user storage
- simple container configuration

Configuration example:

environment variable
EDGEWIT_API_KEY=abc123

Request example:

Authorization: Bearer abc123

Behavior:

- all HTTP endpoints require valid API key
- requests without key return unauthorized response
- ingestion and search endpoints protected equally

Advantages:

- extremely easy to enable
- compatible with automation scripts
- compatible with telemetry collectors
- minimal runtime overhead
- no persistent state required

Use cases:

- multi-application internal network
- lightweight protection for edge nodes
- basic service authentication
- small team deployments

LAYER 3 INFRASTRUCTURE AUTHENTICATION (RECOMMENDED FOR PUBLIC ACCESS)
For deployments that require stronger access control, authentication should be implemented using external infrastructure.

Typical deployment model:

client
↓
reverse proxy or gateway
↓
authentication layer
↓
edgewit

Examples of infrastructure authentication:

- reverse proxy authentication
- service mesh identity
- VPN access control
- gateway API keys
- OAuth via reverse proxy
- cloud edge authentication

Advantages:

- avoids complexity inside Edgewit
- allows flexible authentication strategies
- integrates with existing infrastructure
- keeps Edgewit binary small

LAYER 4 MUTUAL TLS FOR FEDERATED NODES (FUTURE)
When Edgewit nodes operate in a federated configuration, node-to-node authentication will use mutual TLS.

Purpose:

- establish trusted communication between nodes
- prevent unauthorized nodes from joining federation
- encrypt inter-node communication

Characteristics:

- certificate-based node identity
- optional feature enabled via configuration
- primarily used for cluster or federation mode

Typical federation architecture:

edgewit node
↕ mTLS
edgewit node
↕ mTLS
edgewit node

Benefits:

- strong identity verification
- secure distributed queries
- no shared passwords

FEATURE FLAG AND CONFIGURATION STRATEGY

Edgewit maintains a single distributed container image. Security features are enabled via runtime configuration rather than compile-time builds.

Configuration options:

EDGEWIT_API_KEY
enables API key authentication

EDGEWIT_TLS_CERT
server TLS certificate

EDGEWIT_TLS_KEY
server TLS private key

EDGEWIT_FEDERATION_TLS
enables mutual TLS for node federation

This approach ensures:

- container images remain consistent
- no custom builds required
- simple deployment automation
- compatibility with container registries

SECURITY NON-GOALS

Edgewit deliberately does not implement:

- user accounts
- password databases
- role based access control
- OAuth providers
- session management
- identity federation
- authorization policy engines

These responsibilities are intentionally delegated to infrastructure components.

This design keeps the system:

- small
- predictable
- easy to operate on constrained devices

SECURITY RECOMMENDATIONS FOR OPERATORS

Recommended best practices:

1. run Edgewit inside trusted networks
2. use firewall rules to restrict access
3. enable API key authentication when needed
4. place reverse proxy authentication in front of public deployments
5. use VPN access for remote device administration
6. avoid exposing ingestion endpoints directly to the internet
7. rotate API keys periodically in automated environments

BLOG POST OUTLINE
TITLE
Designing Security for Edge Systems: The Edgewit Approach

INTRODUCTION
Introduce Edgewit as a lightweight search and analytics engine for edge devices. Explain that security design for edge infrastructure differs significantly from cloud-scale services.

Explain the challenge: balancing security with simplicity and constrained resources.

ORIGIN OF THE SECURITY MODEL
Describe the initial design constraints:

- Raspberry Pi deployment
- containerized environments
- low memory footprint
- minimal dependencies

Explain why implementing full authentication systems would violate these goals.

EDGE SYSTEMS HAVE DIFFERENT SECURITY ASSUMPTIONS
Explain that edge infrastructure often relies on network trust and infrastructure security.

Discuss examples of common edge deployments:

- factory devices
- internal observability nodes
- home lab systems
- industrial gateways

Highlight how these environments typically rely on network boundaries.

WHY EDGEWIT DOES NOT IMPLEMENT USER MANAGEMENT
Explain the complexity of building user systems.

Discuss typical requirements of user systems:

- identity storage
- credential security
- role management
- password hashing
- token generation
- policy enforcement

Explain how these features significantly increase code size and attack surface.

INFRASTRUCTURE-FIRST SECURITY
Describe the philosophy of delegating authentication to infrastructure.

Provide examples:

- reverse proxies
- service meshes
- network gateways
- VPNs

Explain how this approach simplifies system design.

LIGHTWEIGHT API KEY SUPPORT
Introduce Edgewit’s optional API key authentication.

Explain why this feature exists:

- minimal complexity
- useful for internal services
- easy to configure

Show how it works in practice.

FUTURE NODE SECURITY
Explain future federation features and the use of mutual TLS for node authentication.

Describe why certificates are appropriate for machine-to-machine communication.

BENEFITS OF THIS APPROACH
Summarize advantages:

- smaller binaries
- easier deployment
- reduced attack surface
- flexible integration
- better fit for edge infrastructure

CLOSING THOUGHTS
Reflect on the importance of designing security models that match the deployment environment.

Reinforce that edge infrastructure benefits from simplicity and predictable behavior.

DOCUMENTATION OUTLINE
SECTION
Security Model

OVERVIEW
Explain the Edgewit security philosophy and the assumption of trusted internal deployments.

DEFAULT SECURITY MODEL
Describe trusted network deployments and when this model is appropriate.

API KEY AUTHENTICATION
Explain configuration, usage examples, and recommended scenarios.

INFRASTRUCTURE AUTHENTICATION
Explain how to deploy Edgewit behind reverse proxies and other authentication gateways.

FEDERATED NODE SECURITY
Explain future node-to-node authentication using mutual TLS.

SECURITY BEST PRACTICES
Provide operational guidelines for secure deployments.

SECURITY NON-GOALS
Clarify which authentication features Edgewit intentionally does not implement.

EXAMPLES
Provide example deployment architectures demonstrating secure setups.

SUMMARY
Reinforce the design goal of keeping Edgewit simple, secure, and suitable for constrained edge environments.
