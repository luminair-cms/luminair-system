# About

Luminair is a schema-driven CMS backend written in Rust, exposing a Strapi-like REST API.

Designed by a human.

Implemented with help of AI.

## Deployment

**Primary target**: AWS (Aurora DSQL, ECS / Lambda, IAM auth).

**Also works** in any local, Docker, or Kubernetes environment with a standard PostgreSQL database —
no AWS-specific code leaks into the `domain` or `application` layers.
AWS-specific adapters (DSQL IAM token refresh, etc.) are isolated in the `infrastructure` crate
and are swappable without touching business logic.

For cloud-native production setup with **AWS Cognito & Amazon Aurora DSQL** (architecture notes, production `.env` example, IAM auth token generation, and Cognito JWKS public key extraction), see:

- **[Production Deployment with AWS Cognito & Aurora DSQL (`docs/deploy-with-aws.md`)](./docs/deploy-with-aws.md)**

## Running Locally & REST Debugging

For instructions on running all CLI execution modes (`validate`, `dry-run`, `migrate`, `serve`) via **Docker Compose** or **Cargo**, configuring a local `.env` file, debugging HTTP REST endpoints in the **Zed Editor** with **`httpYac`** and **Dex OIDC**, and running the automated **Testcontainers E2E Component Test Suite**, see:

- **[Local Development, CLI Modes & REST Debugging Guide (`docs/local-development.md`)](./docs/local-development.md)**
- **[E2E Testcontainers Architecture & Implementation Plan (`docs/plan-e2e-testcontainers.md`)](./docs/plan-e2e-testcontainers.md)**

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
| `LUMINAIR_MODE` | `--mode <MODE>` | `validate` | Runtime execution mode: `validate`, `dry-run`, `migrate`, or `serve`. |
| `LUMINAIR_SCHEMA_DIR` | `--schema-dir <DIR>` | `./config/schemas` | Root directory containing `system-config.json` and `document-types/*.json`. |
| `LUMINAIR_DATABASE_URL`<br/>*(or `DATABASE_URL`)* | `--database-url <URL>` | None | Database connection string (PostgreSQL or Amazon Aurora DSQL). Required for `migrate` and `serve` modes. |
| `LUMINAIR_DSQL_AUTH_TOKEN` | — | None | AWS IAM authentication token for Amazon Aurora DSQL connections. |
| `LUMINAIR_MAX_DB_CONNECTIONS` | `--max-db-connections <N>` | `5` | Maximum number of concurrent connections in the SQL pool. |
| `LUMINAIR_SAFETY_POLICY` | `--safety-policy <POLICY>` | `additive` | Dynamic DDL schema migration policy: `additive` or `destructive`. |
| `LUMINAIR_OFFLINE` | `--offline` | `false` | Run without attempting database connectivity (applicable to `validate` and `dry-run` modes). |
| `LUMINAIR_OUTPUT_FORMAT` | `--format <FORMAT>` | `text` | CLI report output format (`text` or `json`). |
| `LUMINAIR_OUTPUT_PATH` | `--output-path <PATH>` | None *(stdout)* | Optional file destination path to write the execution or migration report. |
| `LUMINAIR_HTTP_HOST` | `--http-host <HOST>` | `127.0.0.1` | Network interface address to bind the HTTP REST service in `serve` mode. |
| `LUMINAIR_HTTP_PORT` | `--http-port <PORT>` | `8080` | Port to bind the HTTP REST service in `serve` mode. |
| `LUMINAIR_AUTO_MIGRATE` | `--auto-migrate` | `false` | Automatically apply static migrations and sync dynamic schema on startup in `serve` mode. |
| `LUMINAIR_OIDC_ISSUER` | `--oidc-issuer <URL>` | None | Expected OpenID Connect issuer (`iss` claim), e.g. `http://localhost:5556/dex` or `https://cognito-idp.<region>.amazonaws.com/<user-pool-id>`. |
| `LUMINAIR_OIDC_AUDIENCE` | `--oidc-audience <AUD>` | None | Expected OpenID Connect audience (`aud` claim), e.g. `luminair-admin-spa` or AWS Cognito App Client ID. |
| `LUMINAIR_OIDC_JWKS_URL` | `--oidc-jwks-url <URL>` | None | HTTP endpoint serving the OIDC JSON Web Key Set (JWKS), e.g. `http://dex:5556/dex/keys` or AWS Cognito `.well-known/jwks.json`. |
| `LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM` | `--oidc-jwt-public-key-pem <PEM>` | None | RS256 RSA public key in PEM format used to verify OIDC JWT signatures (e.g. from AWS Cognito). |
| `LUMINAIR_OIDC_JWT_SECRET` | `--oidc-jwt-secret <SECRET>` | None | HMAC-SHA256 (HS256) shared secret for symmetric JWT verification (local development and testing). |
| `LUMINAIR_SERVICE_TOKENS` | `--service-tokens <TOKENS>` | None | Pre-shared machine-to-machine tokens with assigned service ID and roles. Format: `<token>:<service_id>:<roles>;...` or JSON array string. |
| `RUST_LOG` | — | None | Tracing filter directive (e.g. `info`, `debug`, or `infrastructure=debug,domain=info`). |