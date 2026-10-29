# About

Luminair is a schema-driven CMS backend written in Rust, exposing a Strapi-like REST API.

## Division of responsibilities

Designed by a human.

Main code implemented with help of AI (review, analysis, code snippets etc).

Documentation and Tests are written mostly by AI.

## Deployment

**Primary target**: AWS (Aurora DSQL, ECS / Lambda, IAM auth).

**Also works** in any local, Docker, or Kubernetes environment with a standard PostgreSQL database —
no AWS-specific code leaks into the `domain` or `application` layers.
AWS-specific adapters (DSQL IAM token refresh, etc.) are isolated in the `infrastructure` crate
and are swappable without touching business logic.

## CLI Modes & Running with Docker Compose

Luminair provides a CLI (in the `infrastructure` binary) supporting four execution modes using a prepared configuration ([`config/cli-config.json`](./config/cli-config.json)) and declarative schemas ([`config/schemas/`](./config/schemas/)):

| Mode | Description | Requires Database? |
|---|---|---|
| `test` | Loads and validates `cli-config.json`, `system-config.json`, and all `document-types/*.json` schemas into `SystemContext`, outputting a configuration & schema summary report. | No |
| `dry-run` | Loads config and schemas, plans static + dynamic migration steps (either offline against an empty schema with `--offline`, or against a live PostgreSQL database), and outputs all planned steps and generated DDL SQL without modifying the database. | Optional (`--offline` supported) |
| `migrate` | Connects to PostgreSQL, executes embedded static migrations and dynamic schema synchronization, and writes a migration execution report. | Yes |
| `service` | Starts the Axum HTTP REST API service, optionally applies migrations automatically on boot (`autoMigrate`), and serves `/api/v1/*` endpoints. | Yes |

### 1. Run via Docker Compose

[`docker-compose.yml`](./docker-compose.yml) defines background infrastructure services:
- **`postgres`**: PostgreSQL 17 database service on port `5432`.
- **`dex`**: Lightweight Dex OIDC identity provider (Keycloak alternative) on port `5556` configured via [`config/dex.yaml`](./config/dex.yaml).
- **`cli-*`**: Dedicated CLI runner services (`cli` profile) that mount `./config` read-only into `/app/config`.

#### Start Background Services (Postgres & Dex OIDC)
```bash
docker compose up -d postgres dex
# or start all background services:
docker compose up -d
```

**Dex Authentication Provider Details**:
- **Issuer**: `http://localhost:5556/dex`
- **Discovery**: `http://localhost:5556/dex/.well-known/openid-configuration`
- **Pre-seeded Users** (Password: `password`):
  - `admin@luminair.dev` (Subject: `08a1d7f3-admin-sub`)
  - `editor@luminair.dev` (Subject: `08a1d7f3-editor-sub`)
- **Configured Client**: `luminair-admin-spa` (Redirect URIs: `http://localhost:5173/callback`, `http://localhost:3000/callback`)

#### A. Test Mode (Config & Schema Validation — No DB Required)
```bash
docker compose run --rm cli-test
```

#### B. Offline Dry-Run Mode (Preview All Steps & Generated DDL SQL — No DB Required)
```bash
docker compose run --rm cli-dry-run-offline
```

#### C. Live Dry-Run Mode (Diff Schemas Against Running PostgreSQL)
Starts `postgres` automatically, waits until it is healthy, and previews pending static and dynamic migration steps against the live database:
```bash
docker compose run --rm cli-dry-run
```

#### D. Migrate Mode (Apply Static & Dynamic Migrations to PostgreSQL)
Starts `postgres` automatically (if not already running) and applies all static and dynamic migrations:
```bash
docker compose run --rm cli-migrate
```
*(Tip: Run `docker compose run --rm cli-dry-run` again after `cli-migrate` to verify that all static migrations are marked `Applied` and `Dynamic Steps (0)` remain.)*

#### E. Customizing Output Format (`text` vs `json`) or Passing Flags
You can override CLI arguments on any service:
```bash
# Output report as pretty-printed JSON:
docker compose run --rm cli-dry-run-offline \
  --config /app/config/cli-config.json --mode dry-run --offline --format json
```

#### F. Stopping & Resetting the Database
```bash
# Stop containers:
docker compose down

# Stop containers and wipe the PostgreSQL volume to test migrations from scratch:
docker compose down -v
```

### 2. Run Locally with Cargo

You can also run all modes directly from the workspace root without Docker (or against a local/Docker PostgreSQL instance on `localhost:5432`):

```bash
# 1. Validate configuration and schemas:
cargo run -p infrastructure -- --config config/cli-config.json --mode test

# 2. Preview all migration steps and DDL SQL offline:
cargo run -p infrastructure -- --config config/cli-config.json --mode dry-run --offline

# 3. Preview or apply migrations against a running PostgreSQL instance:
docker compose up -d postgres
cargo run -p infrastructure -- --config config/cli-config.json --mode dry-run
cargo run -p infrastructure -- --config config/cli-config.json --mode migrate

# 4. Start the HTTP REST service:
cargo run -p infrastructure -- --config config/cli-config.json --mode service
```

## Environment Variables

Luminair supports full configuration via environment variables alongside CLI arguments and JSON configuration files (such as [`config/cli-config.json`](./config/cli-config.json)).

### Configuration Precedence

Configuration settings are resolved in the following priority order (highest to lowest):
1. **CLI Arguments & Flags** (e.g. `--database-url`, `--mode`, `--auto-migrate`)
2. **Environment Variables** (e.g. `LUMINAIR_DATABASE_URL`, `LUMINAIR_MODE`)
3. **JSON Configuration File** (loaded via `--config <path>` or `LUMINAIR_CONFIG_FILE`)
4. **Built-in Defaults**

### Variable Reference

| Environment Variable | CLI Flag Equivalent | Default | Description |
|---|---|---|---|
| `LUMINAIR_CONFIG_FILE` | `--config <PATH>` | None | Path to base JSON configuration file (e.g. `./config/cli-config.json`). |
| `LUMINAIR_MODE` | `--mode <MODE>` | `test` | Runtime execution mode: `test`, `dry-run`, `migrate`, or `service`. |
| `LUMINAIR_SCHEMA_DIR` | `--schema-dir <DIR>` | `./config/schemas` | Root directory containing `system-config.json` and `document-types/*.json`. |
| `LUMINAIR_DATABASE_URL`<br/>*(or `DATABASE_URL`)* | `--database-url <URL>` | None | Database connection string (PostgreSQL or Amazon Aurora DSQL). Required for `migrate` and `service` modes. |
| `LUMINAIR_DSQL_AUTH_TOKEN` | — | None | AWS IAM authentication token for Amazon Aurora DSQL connections. |
| `LUMINAIR_MAX_DB_CONNECTIONS` | `--max-db-connections <N>` | `5` | Maximum number of concurrent connections in the SQL pool. |
| `LUMINAIR_SAFETY_POLICY` | `--safety-policy <POLICY>` | `additive` | Dynamic DDL schema migration policy: `additive` (`additive-only`) or `destructive` (`allow-destructive`). |
| `LUMINAIR_OFFLINE` | `--offline` | `false` | Run without attempting database connectivity (applicable to `test` and `dry-run` modes). |
| `LUMINAIR_OUTPUT_FORMAT` | `--format <FORMAT>` | `text` | CLI report output format (`text` or `json`). |
| `LUMINAIR_OUTPUT_PATH` | `--output-path <PATH>` | None *(stdout)* | Optional file destination path to write the execution or migration report. |
| `LUMINAIR_HTTP_HOST` | `--http-host <HOST>` | `127.0.0.1` | Network interface address to bind the HTTP REST service in `service` mode. |
| `LUMINAIR_HTTP_PORT` | `--http-port <PORT>` | `8080` | Port to bind the HTTP REST service in `service` mode. |
| `LUMINAIR_AUTO_MIGRATE` | `--auto-migrate` | `false` | Automatically apply static migrations and sync dynamic schema on startup in `service` mode. |
| `LUMINAIR_OIDC_ISSUER` | `--oidc-issuer <URL>` | None | Expected OpenID Connect issuer (`iss` claim), e.g. `http://localhost:5556/dex` or `https://cognito-idp.<region>.amazonaws.com/<user-pool-id>`. |
| `LUMINAIR_OIDC_AUDIENCE` | `--oidc-audience <AUD>` | None | Expected OpenID Connect audience (`aud` claim), e.g. `luminair-admin-spa` or AWS Cognito App Client ID. |
| `LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM` | `--oidc-jwt-public-key-pem <PEM>` | None | RS256 RSA public key in PEM format used to verify OIDC JWT signatures (e.g. from AWS Cognito). |
| `LUMINAIR_OIDC_JWT_SECRET` | `--oidc-jwt-secret <SECRET>` | None | HMAC-SHA256 (HS256) shared secret for symmetric JWT verification (local development and testing). |
| `LUMINAIR_SERVICE_TOKENS` | `--service-tokens <TOKENS>` | None | Pre-shared machine-to-machine tokens with assigned service ID and roles. Format: `<token>:<service_id>:<roles>;...` or JSON array string. |
| `RUST_LOG` | — | None | Tracing filter directive (e.g. `info`, `debug`, or `infrastructure=debug,domain=info`). |

## Example Configurations

### Local Development (`.env`)

For local development, Luminair pairs with a local PostgreSQL 17 database and a lightweight OIDC provider ([Dex](https://dexidp.io/)) managed by [`docker-compose.yml`](./docker-compose.yml).

Save the following as `.env` in the repository root:

```dotenv
# ==============================================================================
# Luminair Local Development Environment (.env)
# ==============================================================================

# Execution Mode & Schema Location
LUMINAIR_MODE=service
LUMINAIR_SCHEMA_DIR=./config/schemas

# HTTP Server Configuration
LUMINAIR_HTTP_HOST=127.0.0.1
LUMINAIR_HTTP_PORT=8080
LUMINAIR_AUTO_MIGRATE=true

# Database (Local PostgreSQL 17 from docker compose)
LUMINAIR_DATABASE_URL=postgres://luminair:luminair@localhost:5432/luminair
LUMINAIR_MAX_DB_CONNECTIONS=5
LUMINAIR_SAFETY_POLICY=additive

# Authentication: Local Dex OIDC Provider (http://localhost:5556/dex)
LUMINAIR_OIDC_ISSUER=http://localhost:5556/dex
LUMINAIR_OIDC_AUDIENCE=luminair-admin-spa
# Note: When neither LUMINAIR_OIDC_JWT_SECRET nor LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM
# is set, Luminair logs a warning and operates in permissive auth mode for testing.
# For HS256 JWT validation, uncomment:
# LUMINAIR_OIDC_JWT_SECRET=local-development-secret-key-min-32-chars

# Machine-to-Machine Service API Tokens (format: <token>:<service_id>:<roles>;...)
# Built-in Roles: admin, editor, writer, viewer
LUMINAIR_SERVICE_TOKENS=local-dev-admin-token:ci-agent:admin;local-dev-editor-token:sync-worker:editor

# Observability
RUST_LOG=info,infrastructure=debug
```

#### Running Locally with `.env`

```bash
# 1. Start background PostgreSQL and Dex services
docker compose up -d postgres dex

# 2. Run the HTTP REST server with environment variables loaded from .env
# (Using export in bash/zsh):
export $(grep -v '^#' .env | xargs) && cargo run -p infrastructure

# 3. Test service health check
curl http://127.0.0.1:8080/healthz
# Returns: {"status":"healthy","database":{"connected":true},"schema":{"documentTypesCount":...}}
```

### Production with AWS Cognito & Aurora DSQL

For cloud-native production deployments on AWS (Amazon ECS Fargate, EKS, or AWS Lambda), Luminair connects directly to **Amazon Aurora DSQL** and verifies tokens issued by **AWS Cognito User Pools**.

#### Architecture Notes

- **Amazon Aurora DSQL ([ADR-003](./docs/adr/ADR-003-aws-dsql-database.md))**:
  - **Endpoint Auto-Detection**: Luminair automatically detects Aurora DSQL from the `*.dsql.<region>.on.aws` hostname or `?dsql=true` query parameter, activating client-side UUIDv7 key generation, standalone non-transactional DDL migrations, and automatic OCC retry loops on SQLSTATE `40001` serialization conflicts.
  - **IAM Authentication**: Aurora DSQL authenticates via short-lived AWS IAM tokens. In containerized environments, pass tokens via `LUMINAIR_DSQL_AUTH_TOKEN` (generated via the AWS CLI or dynamic IAM credential provider) over an encrypted SSL connection (`sslmode=require`).
  - **Connection Pooling**: Aurora DSQL is distributed and serverless; set `LUMINAIR_MAX_DB_CONNECTIONS=20` (or higher) to support concurrent request execution.
- **AWS Cognito Authentication**:
  - **Token Issuer**: `https://cognito-idp.<region>.amazonaws.com/<user-pool-id>`
  - **Audience**: The Cognito App Client ID configured for your application.
  - **Signature Verification**: Cognito issues RS256 JWTs. Extract the RSA public key PEM from Cognito's JWKS endpoint (`https://cognito-idp.<region>.amazonaws.com/<user-pool-id>/.well-known/jwks.json`) and configure `LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM`.
- **Zero-Downtime Schema Safety**: Always use `LUMINAIR_SAFETY_POLICY=additive` in production to prevent destructive table or column drops during dynamic schema synchronization.
- **Migration Orchestration**: In multi-instance or auto-scaling container deployments, set `LUMINAIR_AUTO_MIGRATE=false` on web tasks and run migrations as a dedicated deployment step prior to serving traffic:
  ```bash
  cargo run -p infrastructure -- --mode migrate
  ```

#### Production Environment Example

```dotenv
# ==============================================================================
# Luminair Production Environment (AWS Aurora DSQL & AWS Cognito)
# ==============================================================================

# Execution Mode & Schema Location
LUMINAIR_MODE=service
LUMINAIR_SCHEMA_DIR=/app/config/schemas

# HTTP Server Configuration (Binds to 0.0.0.0 inside container)
LUMINAIR_HTTP_HOST=0.0.0.0
LUMINAIR_HTTP_PORT=8080
# Dedicated migration task runs 'migrate' mode; web instances do not auto-migrate:
LUMINAIR_AUTO_MIGRATE=false

# Database: Amazon Aurora DSQL
# Hostname automatically configures Aurora DSQL pool options (SSL required)
LUMINAIR_DATABASE_URL=postgres://admin@a1b2c3d4e5f6g7h8.dsql.us-east-1.on.aws:5432/postgres?sslmode=require
# Short-lived IAM DB authentication token (generated via AWS SDK or `aws dsql generate-db-connect-admin-auth-token`)
LUMINAIR_DSQL_AUTH_TOKEN=AQICAHh...prod-iam-auth-token...
LUMINAIR_MAX_DB_CONNECTIONS=20
LUMINAIR_SAFETY_POLICY=additive

# Authentication: AWS Cognito User Pool (RS256)
LUMINAIR_OIDC_ISSUER=https://cognito-idp.us-east-1.amazonaws.com/us-east-1_ExamplePoolId
LUMINAIR_OIDC_AUDIENCE=4example7app9client1id234567
# RS256 RSA public key PEM extracted from Cognito JWKS:
LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM="-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA0exampleKey1234567890...\n-----END PUBLIC KEY-----"

# Machine-to-Machine Service API Tokens
# Format: <token>:<service_id>:<role1>,<role2>;...
LUMINAIR_SERVICE_TOKENS="prod-sec-token-9f8a:cloudwatch-monitor:admin;prod-sec-token-3b1c:ingestion-pipeline:editor"

# Observability
RUST_LOG=info,infrastructure=warn
```

#### Generating Aurora DSQL IAM Auth Tokens

To generate a short-lived IAM authentication token using the AWS CLI:

```bash
# Generate admin authentication token for Aurora DSQL cluster (valid for 15 minutes)
export LUMINAIR_DSQL_AUTH_TOKEN=$(aws dsql generate-db-connect-admin-auth-token \
  --region us-east-1 \
  --endpoint a1b2c3d4e5f6g7h8.dsql.us-east-1.on.aws)
```

#### Extracting Cognito RS256 Public Key PEM from JWKS

Cognito exposes JSON Web Keys at `https://cognito-idp.<region>.amazonaws.com/<user-pool-id>/.well-known/jwks.json`. Convert the key into PEM format (for example with `jwk-to-pem` or `openssl`):

```bash
# Fetch and convert JWK to PEM format using node or openssl
npx jwk-to-pem https://cognito-idp.us-east-1.amazonaws.com/us-east-1_ExamplePoolId/.well-known/jwks.json > cognito-public-key.pem
export LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM=$(cat cognito-public-key.pem)
```
