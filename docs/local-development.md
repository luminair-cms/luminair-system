# Local Development, CLI Modes & REST Debugging Guide

This guide covers all local execution modes for Luminair (via **Docker Compose** and **Cargo**) as well as interactive HTTP REST API debugging in the **Zed Editor** using **`httpYac`** and **Dex OIDC**.

---

## 1. CLI Execution Modes Overview

Luminair provides a unified CLI (in the `infrastructure` binary) supporting four execution modes using a prepared configuration ([`config/cli-config.json`](../config/cli-config.json)) and declarative schemas ([`config/schemas/`](../config/schemas/)):

| Mode | Description | Requires Database? |
|---|---|---|
| `validate` | Loads and validates `cli-config.json`, `system-config.json`, and all `document-types/*.json` schemas into `SystemContext`, outputting a configuration & schema summary report. | No |
| `dry-run` | Loads config and schemas, plans static + dynamic migration steps (either offline against an empty schema with `--offline`, or against a live PostgreSQL database), and outputs each planned step with its formatted DDL SQL without modifying the database. | Optional (`--offline` supported) |
| `migrate` | Connects to PostgreSQL, executes embedded static migrations and dynamic schema synchronization, and outputs an execution report with each step and its formatted DDL SQL. | Yes |
| `serve` | Starts the Axum HTTP REST API service, optionally applies migrations automatically on boot (`autoMigrate`), and serves `/api/v1/*` endpoints. | Yes |

---

## 2. Running via Docker Compose

[`docker-compose.yml`](../docker-compose.yml) defines background infrastructure services and dedicated CLI runners:
- **`postgres`**: PostgreSQL 18 (`postgres:18.6-alpine3.24`) database service on port `5432` (volume mounted at `/var/lib/postgresql`).
- **`dex`**: Lightweight Dex OIDC identity provider (`ghcr.io/dexidp/dex:v2.43.0`) on port `5556` configured via [`config/dex.yaml`](../config/dex.yaml).
- **`cli-*`**: Dedicated CLI runner services (`cli` profile) sharing the `luminair-cli:local` image and mounting `./config` read-only into `/app/config`.

### 2.1 Start Background Infrastructure (PostgreSQL & Dex OIDC)

```bash
docker compose up -d postgres dex
# or start all default background services:
docker compose up -d
```

**Dex Authentication Provider Details**:
- **Issuer**: `http://localhost:5556/dex`
- **Discovery**: `http://localhost:5556/dex/.well-known/openid-configuration`
- **Token Endpoint**: `http://localhost:5556/dex/token` (supports `grant_type=password` via `oauth2.passwordConnector: local`)
- **Pre-seeded Users** (Password: `password`):
  - `admin@luminair.dev` (Subject: `08a1d7f3-admin-sub`)
  - `editor@luminair.dev` (Subject: `08a1d7f3-editor-sub`)
- **Configured Client**: `luminair-admin-spa` (Redirect URIs: `http://localhost:5173/callback`, `http://localhost:3000/callback`)

### 2.2 Run CLI Modes in Docker

#### A. Validate Mode (Config & Schema Validation — No DB Required)
```bash
docker compose run --rm cli-validate
```

#### B. Offline Dry-Run Mode (Preview All Steps & Formatted DDL SQL — No DB Required)
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

#### E. Customizing Output Format (`text` vs `json`) or Rebuilding the CLI Image
```bash
# Rebuild the shared luminair-cli:local image after code changes:
docker compose build cli-migrate

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

---

## 3. Running Locally with Cargo & `.env`

You can run all modes directly from the workspace root using Cargo against the Docker PostgreSQL and Dex containers (`localhost:5432` and `localhost:5556`):

```bash
# 1. Validate configuration and schemas:
cargo run -p infrastructure -- --config config/cli-config.json --mode validate

# 2. Preview all migration steps and formatted DDL SQL offline:
cargo run -p infrastructure -- --config config/cli-config.json --mode dry-run --offline

# 3. Preview or apply migrations against a running PostgreSQL instance:
docker compose up -d postgres
cargo run -p infrastructure -- --config config/cli-config.json --mode dry-run
cargo run -p infrastructure -- --config config/cli-config.json --mode migrate

# 4. Start the HTTP REST service (with auto-migrate and bootstrap service token):
docker compose up -d postgres dex
cargo run -p infrastructure -- \
  --config config/cli-config.json \
  --mode serve \
  --auto-migrate \
  --service-tokens "local-dev-admin-token:ci-agent:admin"
```

### Local Development `.env` Configuration

Alternatively, save the following as `.env` in the repository root:

```dotenv
# ==============================================================================
# Luminair Local Development Environment (.env)
# ==============================================================================

# Execution Mode & Schema Location
LUMINAIR_MODE=serve
LUMINAIR_SCHEMA_DIR=./config/schemas

# HTTP Server Configuration
LUMINAIR_HTTP_HOST=127.0.0.1
LUMINAIR_HTTP_PORT=8080
LUMINAIR_AUTO_MIGRATE=true

# Database (Local PostgreSQL from docker compose)
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

#### Running with `.env`
```bash
# 1. Start background PostgreSQL and Dex services
docker compose up -d postgres dex

# 2. Run the HTTP REST server with environment variables loaded from .env
# (Using export in bash/zsh):
export $(grep -v '^#' .env | xargs) && cargo run -p infrastructure

# 3. Verify service health check
curl http://127.0.0.1:8080/healthz
```

---

## 4. Debugging HTTP REST Endpoints in Zed Editor (`httpYac` + Dex OIDC)

Zed supports interactive `.http` request collections with clickable gutter `▷` run buttons by combining the **`http`** syntax extension with **[httpYac](https://httpyac.github.io/)** and **Zed Tasks**.

### 4.1 Prerequisites (What Must Be Installed)

1. **Zed Editor Extension**:
   - Open Zed Command Palette (`Ctrl+Shift+P` on Windows/Linux, `Cmd+Shift+P` on macOS) $\rightarrow$ **`zed: extensions`**.
   - Search for and install **`http`** (provides Tree-sitter syntax highlighting and runnable block detection for `.http` and `.rest` files) or **`REST Client`**.
2. **httpYac CLI**:
   - Requires Node.js (`v18+`). Install `httpyac` globally:
     ```bash
     npm install -g httpyac
     ```
   - Verify installation:
     ```bash
     httpyac --version
     ```

### 4.2 Zed Tasks Configuration (`.zed/tasks.json`)

Create `.zed/tasks.json` in the workspace root. This wires Zed's task runner to `httpyac` using Zed's `$ZED_FILE` and `$ZED_ROW` variables so you can execute the specific request under your cursor:

```json
[
  {
    "label": "httpYac: Send Request at Cursor",
    "command": "httpyac",
    "args": ["send", "$ZED_FILE", "--line", "$ZED_ROW", "--output", "body"],
    "reveal": "always",
    "use_new_terminal": false
  },
  {
    "label": "httpYac: Send Request (Full Headers + Body)",
    "command": "httpyac",
    "args": ["send", "$ZED_FILE", "--line", "$ZED_ROW", "--output", "exchange"],
    "reveal": "always",
    "use_new_terminal": false
  },
  {
    "label": "httpYac: Run All Requests in File",
    "command": "httpyac",
    "args": ["send", "$ZED_FILE", "--all"],
    "reveal": "always",
    "use_new_terminal": false
  }
]
```

### 4.3 HTTP Request Collection (`.ai/local/luminair-dex.http`)

Create `.ai/local/luminair-dex.http` (`.ai/local/` is gitignored so local tokens and scratch files are never committed).

This collection uses `httpyac`'s `# @name` and `# @ref` annotations to:
1. Acquire a real OIDC `id_token` directly from the running **Dex** container (`POST http://localhost:5556/dex/token`).
2. Onboard the Dex user into Luminair (`POST /api/v1/auth/access-requests`) and approve the request (`POST /api/v1/auth/access-requests/:id/approve`) using the bootstrap `local-dev-admin-token`.
3. Call Luminair's content, schema, and system endpoints authenticated as the Dex OIDC user.

```http
@baseUrl = http://localhost:8080
@dexUrl = http://localhost:5556/dex
@clientId = luminair-admin-spa

# Bootstrap M2M service token (used once to approve the first human user's onboarding request)
@serviceToken = local-dev-admin-token

# Built-in role UUIDs from infrastructure/src/persistence/migration/embedded.rs
@adminRoleId = 01920000-0000-7000-8000-000000000001
@editorRoleId = 01920000-0000-7000-8000-000000000002

### ============================================================================
### 1. Dex OIDC Discovery & Token Acquisition (Port 5556)
### ============================================================================

### 1a. Check Dex OpenID Configuration
GET {{dexUrl}}/.well-known/openid-configuration
Accept: application/json

### 1b. Obtain OIDC Token from Dex for admin@luminair.dev
# @name dexAdminLogin
POST {{dexUrl}}/token
Content-Type: application/x-www-form-urlencoded

grant_type=password
&scope=openid+profile+email
&client_id={{clientId}}
&username=admin@luminair.dev
&password=password

### 1c. Obtain OIDC Token from Dex for editor@luminair.dev
# @name dexEditorLogin
POST {{dexUrl}}/token
Content-Type: application/x-www-form-urlencoded

grant_type=password
&scope=openid+profile+email
&client_id={{clientId}}
&username=editor@luminair.dev
&password=password


### ============================================================================
### 2. Onboarding the Dex User into Luminair (Run Once per Fresh DB)
### ============================================================================

### 2a. Sync Dex User Claims (email, name) into Local User Directory
# @ref dexAdminLogin
POST {{baseUrl}}/api/v1/auth/profile/sync
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Accept: application/json

### 2b. Submit Onboarding Access Request as Dex User
# @ref dexAdminLogin
# @name submitAccessRequest
POST {{baseUrl}}/api/v1/auth/access-requests
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Accept: application/json

### 2c. Approve Access Request & Assign Admin Role (using bootstrap Service Token)
# @ref submitAccessRequest
POST {{baseUrl}}/api/v1/auth/access-requests/{{submitAccessRequest.response.body.data.id}}/approve
Authorization: Bearer {{serviceToken}}
Content-Type: application/json

{
  "roleIds": ["{{adminRoleId}}"]
}


### ============================================================================
### 3. Call Luminair APIs Authenticated as the Dex OIDC User
### ============================================================================

### 3a. Health & Readiness Probes
GET {{baseUrl}}/ready
Accept: application/json

### 3b. Get System Config (Authenticated via Dex JWT)
# @ref dexAdminLogin
GET {{baseUrl}}/api/v1/system/config
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Accept: application/json

### 3c. List Registered Document Type Schemas
# @ref dexAdminLogin
GET {{baseUrl}}/api/v1/schema/document-types
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Accept: application/json

### 3d. Create an Article Draft as Dex User
# @ref dexAdminLogin
# @name createArticle
POST {{baseUrl}}/api/v1/content/article
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Content-Type: application/json

{
  "fields": {
    "title": "Authenticated via Dex OIDC",
    "slug": "authenticated-via-dex-oidc",
    "summary": { "en": "Created from Zed using httpYac and Dex" },
    "priority": 10
  },
  "relations": {}
}

### 3e. Publish the Article
# @ref dexAdminLogin
# @ref createArticle
POST {{baseUrl}}/api/v1/content/article/{{createArticle.response.body.data.id}}/publish
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Content-Type: application/json

### 3f. Query Published Articles
# @ref dexAdminLogin
GET {{baseUrl}}/api/v1/content/article
Authorization: Bearer {{dexAdminLogin.response.body.id_token}}
Accept: application/json
```

### 4.4 Step-by-Step: How to Use It in Zed

1. **Start the Services**:
   - Run `docker compose up -d postgres dex` to start PostgreSQL and Dex.
   - Start `luminair-service`:
     ```powershell
     cargo run -p infrastructure -- --config config/cli-config.json --mode serve --auto-migrate --service-tokens "local-dev-admin-token:ci-agent:admin"
     ```
2. **Open `.ai/local/luminair-dex.http` in Zed**.
3. **Execute a Request**:
   - Place your cursor anywhere inside a `###` request block and either:
     - Click the **`▷`** play icon in the editor gutter and choose **`httpYac: Send Request at Cursor`** (or **`httpYac: Send Request (Full Headers + Body)`**), OR
     - Press **`Alt+Shift+T`** (Zed's `task: spawn` shortcut) and select the task, OR
     - Press **`Ctrl+Alt+R`** to re-run the last executed task.
4. **First-Time User Onboarding (Steps `2a` $\rightarrow$ `2b` $\rightarrow$ `2c`)**:
   - On a fresh database, a human OIDC user (`admin@luminair.dev`) does not yet have a role assignment in PostgreSQL (calling protected endpoints returns `403 Forbidden` with `"code": "access_not_requested"`).
   - Run **`2a`** (syncs user profile), **`2b`** (submits access request and captures its ID via `# @name submitAccessRequest`), and **`2c`** (uses `@serviceToken` to approve the request and grant `@adminRoleId`).
5. **Automatic Token & ID Chaining (`# @ref`)**:
   - Any request annotated with `# @ref dexAdminLogin` automatically executes `dexAdminLogin` first and injects `{{dexAdminLogin.response.body.id_token}}` into the `Authorization: Bearer` header.
   - Similarly, step **`3e`** (`Publish the Article`) references `# @ref createArticle` and uses `{{createArticle.response.body.data.id}}` from step **`3d`**.


---

## 5. Automated End-to-End (E2E) Component Testing with Testcontainers

In addition to manual REST debugging via `httpYac`, Luminair includes a fully automated container-to-container E2E component test suite ([`infrastructure/tests/e2e_service_test.rs`](../infrastructure/tests/e2e_service_test.rs)) powered by [`testcontainers`](https://docs.rs/testcontainers) (`0.28.0`), [`reqwest`](https://docs.rs/reqwest) (`0.13.5`), and [`assert-json-diff`](https://docs.rs/assert-json-diff) (`2.0.2`).

### 5.1 Container Topology & What It Verifies

When a local Docker daemon is available, `E2eEnvironment::start()` ([`infrastructure/tests/e2e_support/containers.rs`](../infrastructure/tests/e2e_support/containers.rs)) programmatically provisions an isolated Docker bridge network (`luminair-e2e-<uuid_v7>`) and orchestrates three containers:

1. **PostgreSQL (`postgres:18.6-alpine3.24`)** — target relational database.
2. **Dex OIDC (`ghcr.io/dexidp/dex:v2.43.0`)** — live OpenID Connect identity provider mounted with [`config/dex.yaml`](../config/dex.yaml).
3. **Luminair Service (`luminair-cli:local`)** — compiled Docker image running in `--mode serve --auto-migrate` with live RS256 JWKS verification (`LUMINAIR_OIDC_JWKS_URL=http://<dex_alias>:5556/dex/keys`).

The suite exercises the complete production HTTP stack over real TCP sockets:
- **Health & OIDC JWKS Verification**: `/healthz`, `/ready`, live RS256 ID token acquisition from Dex (`admin@luminair.dev` and `editor@luminair.dev`), and forged token rejection (`401 unauthenticated`).
- **User Onboarding & Directory**: Pre-onboarding guard (`403 access_not_requested`), profile sync (`/api/v1/auth/profile/sync`), access request submission (`403 access_pending`), M2M bootstrap approval (`local-dev-admin-token`), delegated human role approval, and user directory search (`/api/v1/auth/users`).
- **Single-Lifecycle Collections (`category`, `tag`)**: Immediate publication (`draftAndPublish: false`), lifecycle guards (`409 draft_and_publish_disabled`), update with version increment, and deletion (`204 No Content` $\rightarrow$ `404 Not Found`).
- **Singleton Lifecycle (`site-settings`)**: Draft creation, singleton uniqueness guard (`409 singleton_already_exists`), revision 1 publish, `modified` state transitions, `?view=published` vs `?view=working` isolation, and revision 2 publish.
- **Full Collection Lifecycle, Relations & Snapshots (`article`)**: All field types (`text`, `uid`, `localizedText`, `int32`, `decimal`, `json`), OCC conflict detection (`409 version_mismatch`), relational mutations (`set`, `connect`, `disconnect`), `?populate=category,tags`, revision snapshots (`/snapshots`), conflict-guarded snapshot restore (`409 draft_has_pending_edits` and `discardPendingEdits: true`), `/discard-draft`, `/unpublish`, and `DELETE`.

### 5.2 Running the E2E Component Test Suite

**PowerShell (Windows):**
```powershell
# Run the full E2E Testcontainers suite (builds luminair-cli:local automatically):
cargo test -p infrastructure --test e2e_service_test -- --nocapture

# Skip rebuilding the Docker image on subsequent runs when only test code changed:
$env:LUMINAIR_SKIP_DOCKER_BUILD="1"; cargo test -p infrastructure --test e2e_service_test -- --nocapture

# Clear the variable in the current PowerShell session when you want to rebuild again:
Remove-Item Env:LUMINAIR_SKIP_DOCKER_BUILD -ErrorAction SilentlyContinue
```

**Bash / Zsh (Linux / macOS / Git Bash):**
```bash
# Run the full E2E Testcontainers suite (builds luminair-cli:local automatically):
cargo test -p infrastructure --test e2e_service_test -- --nocapture

# Skip rebuilding the Docker image on subsequent runs when only test code changed:
LUMINAIR_SKIP_DOCKER_BUILD=1 cargo test -p infrastructure --test e2e_service_test -- --nocapture
```

*(Note: If the Docker daemon is not accessible in the current environment, `test_e2e_full_service_lifecycle` logs a notice and skips gracefully so offline `cargo test --workspace` runs always succeed. See [`docs/plan-e2e-testcontainers.md`](./plan-e2e-testcontainers.md) for the full architectural specification.)*
