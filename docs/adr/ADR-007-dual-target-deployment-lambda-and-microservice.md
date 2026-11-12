# ADR-007: Dual-Target Deployment (Microservice & AWS Lambda) with Edge Cognito Auth and Granular Infrastructure Crates

- **Status**: Accepted
- **Date**: 2026-11-12 (proposed) · 2026-11-12 (accepted)
- **Deciders**: Dmitri Astafiev
- **Governing decisions**: [`ADR-002`](./ADR-002-hexagonal-architecture.md), [`ADR-003`](./ADR-003-aws-dsql-database.md), [`ADR-006`](./ADR-006-user-profile-and-directory-architecture.md)
- **Related docs**: [`docs/architecture.md`](../architecture.md), [`docs/deploy-with-aws.md`](../deploy-with-aws.md), [`docs/refactoring/refactoring-dual-target-lambda-and-microservice.md`](../refactoring/refactoring-dual-target-lambda-and-microservice.md)

---

## Context

Luminair was originally architected and packaged as a 3-crate Cargo workspace (`domain`, `application`, `infrastructure`), where the `infrastructure` crate functioned as both an all-inclusive adapter library (HTTP router, database persistence, in-process OIDC auth, schema loader) and the sole binary entrypoint (`infrastructure/src/main.rs`).

As the CMS backend evolved to support diverse production environments on AWS, two major deployment paradigms emerged:
1. **Long-Running Container Microservices**: High-throughput daemons running on Amazon ECS Fargate, EKS, Docker, or EC2 with persistent TCP sockets (`axum::serve`), in-process OIDC token validation, and multi-connection database pooling.
2. **Serverless Event-Driven Functions**: Scale-to-zero functions running on AWS Lambda (`provided.al2023`, `arm64` Graviton) fronted by Amazon API Gateway HTTP API (v2) and backed by Amazon Aurora DSQL.
3. **Deterministic Administrative / Migration Pipelines**: Standalone CLI execution for schema validation, offline dry-run inspection, and dynamic DDL migrations in CI/CD pipelines.

### The Architectural Challenges

1. **Monolithic Infrastructure Bloat & Attack Surface**:
   Compiling a single binary for all environments meant that a lightweight migration CLI (`luminair-cli`) pulled in Axum, Tokio HTTP machinery, JSON Web Token cryptography (`jsonwebtoken`), and AWS Lambda dependencies. Conversely, a serverless Lambda binary included CLI argument parsers and daemon TCP socket listeners.
2. **The Cold-Start & In-Process Cryptography Penalty on AWS Lambda**:
   In-process OIDC token validation requires running CPU-intensive RSA signature verification (`RS256`) on every request, adding 5–15ms of billable execution duration per invocation. Furthermore, unauthenticated or malicious spam requests invoke the Lambda function, incurring compute costs solely to return `401 Unauthorized`.
3. **Optimistic Concurrency Control (OCC) Write Contention on Aurora DSQL**:
   AWS Aurora DSQL (ADR-003) is an Active-Active distributed database where concurrent writes to identical records encounter SQLSTATE `40001` serialization conflicts. Per ADR-006, identity resolution must be strictly read-only on the request hot path. If edge authentication implicitly triggers database writes, distributed read traffic will trigger severe OCC lock contention.
4. **Serverless Connection Sizing Exhaustion**:
   In AWS Lambda, hundreds or thousands of execution sandboxes scale horizontally in response to incoming traffic. If each sandbox allocates a traditional connection pool (e.g., 20 connections), the database connection capacity will be rapidly exhausted.
5. **Dynamic Migration Cold-Start Hazards**:
   Executing dynamic DDL migrations or introspections during Lambda cold starts causes distributed race conditions, severe latency spikes, and OCC deadlocks when multiple Lambda sandboxes initialize concurrently.

---

## Considered Alternatives

### Alternative 1: Monolithic Binary with Runtime Flags

Maintain a single binary in `infrastructure/src/main.rs` that switches behavior via runtime environment variables (e.g. `LUMINAIR_TARGET=lambda` vs `LUMINAIR_TARGET=server`).

- ❌ **Massive Binary Size & Slow Cold Starts**: Packaging web daemons, CLI tools, and Lambda runtimes into a single binary inflates artifact size, slowing container image pulls and Lambda initialization.
- ❌ **Wasted Memory & Dependencies**: A simple migration task pulls in the entire web server and cryptographic stack.
- ❌ **Fragile Runtime Branching**: Complex branching in `main()` increases the risk of misconfiguration in production.

### Alternative 2: Cargo Feature Flags in a Single Crate

Keep a single `infrastructure` crate and use Cargo feature flags (e.g. `--features lambda` or `--features microservice`) to toggle dependencies.

- ❌ **Leaky Abstractions & Feature Unification**: Cargo unifies features across workspace members. If one crate or test activates a feature, it leaks into all builds within the workspace.
- ❌ **Complex Conditional Compilation**: Codebases littered with `#[cfg(feature = "...")]` attributes become difficult to read, navigate, and test reliably.
- ❌ **Inflexible Tooling**: Does not allow building both the CLI and server binary simultaneously with different dependency subsets.

### Alternative 3: Granular Infrastructure Adapter Crates with Dedicated Platform Entrypoints (Selected)

Decompose the infrastructure layer into focused, single-responsibility adapter crates (`infrastructure/*`) and standalone execution entrypoint binaries (`bootstrap/*`). Combine this with Amazon API Gateway HTTP API (v2) edge JWT validation and parameter mappings for AWS Lambda.

- ✅ **True Dependency & Compile-Time Isolation**: `luminair-cli` depends only on persistence and schema loading (zero HTTP/Lambda dependencies). `aws-lambda` compiles only the HTTP routing and edge claim resolution (zero in-process RSA/JWKS cryptography).
- ✅ **Zero-Cost Edge Defense**: Unauthenticated requests are rejected at the AWS network edge by API Gateway ($0 Lambda invocation cost).
- ✅ **Zero-Cryptography Lambda Execution**: Avoids RSA signature verification inside Lambda, cutting execution duration by 5–15ms per request.
- ✅ **Clean Hexagonal Architecture**: Domain and application layers remain completely untouched; adapters strictly adhere to their respective ports.

---

## Decision

We adopt **Alternative 3**. We establish a dual-target architecture with granular adapter crates, dedicated platform bootstrap binaries, edge-authenticated Cognito authorization, serverless connection pool sizing, and strict migration separation.

### Decision 1: Crate Layout — Granular `infrastructure/*` & Isolated `bootstrap/*`

The workspace is organized into core logic, granular adapter libraries, and dedicated platform entrypoints:

```text
luminair-system/
├── Cargo.toml                              # Workspace root manifest
├── domain/                                 # Pure domain entities, value objects, ports (unchanged)
├── application/                            # Pure use-cases, services (unchanged)
│
├── infrastructure/                         # Adapter layer (granular library crates)
│   ├── schema-loader/                      # Schema disk walker, validation & SystemContext assembly
│   ├── persistence/                        # Database adapters, SeaQuery, SQLx, DSQL connection factory
│   ├── auth/                               # In-process OIDC validator, JWKS client & Service Tokens
│   ├── auth-cognito-edge/                  # Edge-authenticated claims resolver (zero cryptography)
│   └── http/                               # Axum router, REST routes, handlers, DTO codecs, ProblemDetails
│
└── bootstrap/                              # Platform execution entrypoints (binaries)
    ├── aws-lambda/                         # AWS Lambda custom runtime (`bootstrap` binary)
    ├── luminair-server/                    # Long-running HTTP service daemon (`luminair-server` binary)
    └── luminair-cli/                       # Administrative & migration CLI (`luminair-cli` binary)
```

#### Dependency Matrix:

| Crate | Category | Depends On | Excludes |
|---|---|---|---|
| `luminair-schema-loader` | Adapter | `domain`, `serde`, `serde_json`, `indexmap` | DB, HTTP, Lambda |
| `luminair-persistence` | Adapter | `domain`, `application`, `luminair-schema-loader`, `sqlx`, `sea-query` | Axum, Lambda, OIDC |
| `luminair-auth` | Adapter | `domain`, `application`, `luminair-persistence`, `jsonwebtoken`, `reqwest` | Axum, Lambda |
| `luminair-auth-cognito-edge` | Adapter | `domain`, `application`, `luminair-persistence` | `jsonwebtoken`, `reqwest`, Axum, Lambda |
| `luminair-http` | Adapter | `domain`, `application`, `luminair-schema-loader`, `luminair-persistence`, `axum` | `jsonwebtoken`, `reqwest`, Lambda |
| `bootstrap/luminair-cli` | Binary | `domain`, `application`, `luminair-schema-loader`, `luminair-persistence` | Axum, HTTP, Lambda, OIDC |
| `bootstrap/luminair-server` | Binary | `domain`, `application`, `luminair-schema-loader`, `luminair-persistence`, `luminair-auth`, `luminair-http` | Lambda |
| `bootstrap/aws-lambda` | Binary | `domain`, `application`, `luminair-schema-loader`, `luminair-persistence`, `luminair-auth-cognito-edge`, `luminair-http`, `lambda_http` | `luminair-auth`, `jsonwebtoken`, `reqwest` |

---

### Decision 2: AWS Cognito Edge Authentication with Decoupled Credentials

To support both in-process token validation (microservices) and edge-validated claims (Lambda) without leaking transport concepts into domain ports:

1. **Transport-Agnostic Credential Model (`AuthCredentials`)**:
   Defined in `luminair_http::auth`:
   ```rust
   pub enum AuthCredentials<'a> {
       /// In-process token validation (Bearer JWT or Service Token)
       BearerToken(&'a str),
       /// Verified edge claims injected by API Gateway
       EdgeCognito {
           user_id: &'a str,
           email: Option<&'a str>,
           name: Option<&'a str>,
       },
   }
   ```
2. **Amazon API Gateway HTTP API (v2) JWT Authorizer + Parameter Mappings**:
   - API Gateway verifies Cognito JWT signatures (`RS256`), expiration (`exp`), issuer (`iss`), and audience (`aud`) at the AWS network edge.
   - Unauthenticated or expired requests are rejected at the edge with `401 Unauthorized`. **Lambda is NOT invoked (Cost = $0)**.
   - Valid requests overwrite headers via Parameter Mappings:
     ```yaml
     ParameterMappings:
       - Header:
           Name: x-cognito-sub
           Mapping: $context.authorizer.jwt.claims.sub
       - Header:
           Name: x-cognito-email
           Mapping: $context.authorizer.jwt.claims.email
       - Header:
           Name: x-cognito-name
           Mapping: $context.authorizer.jwt.claims.name
     ```
   - **Spoofing Prevention**: Parameter mappings explicitly overwrite any client-supplied headers at the AWS edge network before reaching Lambda.

---

### Decision 3: Monomorphized Composition Root with RPITIT

To avoid dynamic heap allocations (`Box<dyn ...>`) and runtime vtable dispatch in high-performance web and serverless pipelines, the `AuthContextResolver` port trait employs modern Rust Return-Position Impl Trait in Trait (RPITIT):

```rust
pub trait AuthContextResolver: Send + Sync + 'static {
    fn resolve_user_identity(
        &self,
        credentials: AuthCredentials<'_>,
    ) -> impl Future<Output = Result<AuthenticatedUser, AuthResolutionError>> + Send;

    fn resolve_caller(
        &self,
        credentials: AuthCredentials<'_>,
    ) -> impl Future<Output = Result<CallerContext, AuthResolutionError>> + Send;
}
```

The composition root `AppContainer<Auth>` and web state `HttpState` are generic over `Auth: AuthContextResolver`:
- In `luminair-server`: Monomorphized with `SqlxAuthResolver`.
- In `aws-lambda`: Monomorphized with `CognitoEdgeAuthResolver`.
- Both entrypoints compile to direct, inlined static function calls with zero runtime dynamic dispatch overhead.

---

### Decision 4: ADR-006 Conformance (Strictly Read-Only `CognitoEdgeAuthResolver`)

In adherence to ADR-006:
- `CognitoEdgeAuthResolver` reads `user_role_assignments` and `access_requests` to construct `CallerContext`.
- It executes **zero write operations** on the authentication request path, completely protecting Amazon Aurora DSQL from Optimistic Concurrency Control (OCC `40001`) serialization conflicts.
- User profile synchronization is decoupled from the hot-path request pipeline.

---

### Decision 5: Aurora DSQL Serverless Pool Sizing & Dynamic IAM Refresh

1. **Serverless Pool Sizing Safeguards**:
   When running inside an AWS Lambda sandbox (detected via `AWS_LAMBDA_FUNCTION_NAME`), `create_connection_pool` clamps `max_connections` to `1` (clamped to an absolute maximum of 2).
   - In container microservice environments, `max_connections` defaults to 20–50.
   - Clamping prevents distributed connection starvation across hundreds of concurrently scaled Lambda sandboxes.
2. **Dynamic IAM Token Refresh**:
   Aurora DSQL IAM tokens expire every 15 minutes. `AwsSdkDsqlTokenProvider` automatically generates fresh tokens, while SQLx pool recycling (`max_lifetime = 10m`, `idle_timeout = 5m`) guarantees connections close well before tokens expire.

---

### Decision 6: Bundled Schemas per Project Artifact

Luminair operates as a project-agnostic CMS engine:
- Custom domain models are declared in JSON schemas (`system-config.json` and `document-types/*.json`).
- Deployment artifacts package schemas directly:
  - In AWS Lambda: Bundled at `/var/task/schemas` (configured via `LUMINAIR_SCHEMA_DIR=/var/task/schemas`).
  - In Docker: Bundled at `/app/schemas` or mounted via volume.
- During cold start, schemas are validated and converted into an immutable `&'static SystemContext` with zero runtime allocation.

---

### Decision 7: Strict Migration Lifecycle Separation

Dynamic database schema migrations and static SQLx migrations must **never** run on Lambda cold starts:
- Running migrations on cold starts introduces race conditions across concurrent sandboxes, triggers OCC deadlocks on Aurora DSQL, and degrades cold-start latency.
- Migrations are decoupled into explicit pre-deployment stages using `luminair-cli`:
  1. `luminair-cli --mode validate`: Verifies schema JSON integrity offline.
  2. `luminair-cli --mode dry-run`: Emits planned SQL DDL without database mutation.
  3. `luminair-cli --mode migrate`: Executes static and dynamic migrations prior to shifting traffic.

---

## Consequences

### Positive
- **Optimal Price-Performance on AWS Lambda**: Unauthenticated requests are rejected at the edge with $0 Lambda cost; eliminating in-process RSA signature checks reduces execution duration by 5–15ms per invocation.
- **Minimal Lambda Artifact Size & Rapid Cold Starts**: The `bootstrap` binary is stripped of unused CLI and in-process OIDC dependencies, resulting in minimal cold-start initialization.
- **Fast CLI Tooling**: `luminair-cli` compiles in seconds and carries zero web or Lambda dependencies.
- **Zero-Cost Monomorphization**: RPITIT avoids dynamic trait object overhead (`dyn Trait`) in high-frequency request paths.
- **Database Safety**: Enforced serverless connection pool sizing (`max_connections = 1`) and read-only authentication resolve connection starvation and OCC conflicts on Aurora DSQL.
- **Project Agnostic**: Schemas are declared purely in JSON and bundled into artifacts without altering Rust application code.

### Negative / Trade-Offs
- **Multiple Crate Management**: Managing 5 granular adapter crates and 3 binary crates increases the number of `Cargo.toml` files in the workspace (mitigated by workspace-level dependency inheritance in root `Cargo.toml`).
- **AWS API Gateway Parameter Mapping Dependency**: Lambda edge authentication relies on API Gateway HTTP API (v2) parameter mapping configuration. Running Lambda directly via unmapped Function URLs without an API Gateway authorizer will result in unauthenticated rejections.

---

## References

- [`docs/architecture.md`](../architecture.md): System architecture and crate responsibilities.
- [`docs/deploy-with-aws.md`](../deploy-with-aws.md): AWS Lambda and ECS production deployment guide.
- [`docs/refactoring/refactoring-dual-target-lambda-and-microservice.md`](../refactoring/refactoring-dual-target-lambda-and-microservice.md): Dual-target implementation plan.
- [`ADR-002: Hexagonal Architecture`](./ADR-002-hexagonal-architecture.md).
- [`ADR-003: Amazon Aurora DSQL Database Integration`](./ADR-003-aws-dsql-database.md).
- [`ADR-006: User Profile and Directory Architecture`](./ADR-006-user-profile-and-directory-architecture.md).
