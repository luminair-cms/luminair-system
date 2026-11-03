# Production Deployment with AWS Cognito & Aurora DSQL

For cloud-native production deployments on AWS (Amazon ECS Fargate, EKS, or AWS Lambda), Luminair connects directly to **Amazon Aurora DSQL** and verifies tokens issued by **AWS Cognito User Pools**.

---

## 1. Architecture Notes

- **Amazon Aurora DSQL ([ADR-003](./adr/ADR-003-aws-dsql-database.md))**:
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

---

## 2. Production Environment Example

```dotenv
# ==============================================================================
# Luminair Production Environment (AWS Aurora DSQL & AWS Cognito)
# ==============================================================================

# Execution Mode & Schema Location
LUMINAIR_MODE=serve
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

---

## 3. Generating Aurora DSQL IAM Auth Tokens

To generate a short-lived IAM authentication token using the AWS CLI:

```bash
# Generate admin authentication token for Aurora DSQL cluster (valid for 15 minutes)
export LUMINAIR_DSQL_AUTH_TOKEN=$(aws dsql generate-db-connect-admin-auth-token \
  --region us-east-1 \
  --endpoint a1b2c3d4e5f6g7h8.dsql.us-east-1.on.aws)
```

---

## 4. Extracting Cognito RS256 Public Key PEM from JWKS

Cognito exposes JSON Web Keys at `https://cognito-idp.<region>.amazonaws.com/<user-pool-id>/.well-known/jwks.json`. Convert the key into PEM format (for example with `jwk-to-pem` or `openssl`):

```bash
# Fetch and convert JWK to PEM format using node or openssl
npx jwk-to-pem https://cognito-idp.us-east-1.amazonaws.com/us-east-1_ExamplePoolId/.well-known/jwks.json > cognito-public-key.pem
export LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM=$(cat cognito-public-key.pem)
```
