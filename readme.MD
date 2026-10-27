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

Luminair provides a CLI (in the `infrastructure` binary) supporting three execution modes using a prepared configuration ([`config/cli-config.json`](./config/cli-config.json)) and declarative schemas ([`config/schemas/`](./config/schemas/)):

| Mode | Description | Requires Database? |
|---|---|---|
| `test` | Loads and validates `cli-config.json`, `system-config.json`, and all `document-types/*.json` schemas into `SystemContext`, outputting a configuration & schema summary report. | No |
| `dry-run` | Loads config and schemas, plans static + dynamic migration steps (either offline against an empty schema with `--offline`, or against a live PostgreSQL database), and outputs all planned steps and generated DDL SQL without modifying the database. | Optional (`--offline` supported) |
| `migrate` | Connects to PostgreSQL, executes embedded static migrations and dynamic schema synchronization, and writes a migration execution report. | Yes |

### 1. Run via Docker Compose

[`docker-compose.yml`](./docker-compose.yml) defines a PostgreSQL 17 service (`postgres`) and dedicated CLI runner services (`cli` profile) that mount `./config` read-only into `/app/config`.

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
```
