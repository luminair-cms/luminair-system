# Production Deployment with AWS Cognito & Aurora DSQL

Luminair provides first-class support for cloud-native production deployments on AWS, targeting either **Serverless AWS Lambda** (Graviton `arm64`) or **Container Microservices** (Amazon ECS Fargate / EKS / EC2), backed by **Amazon Aurora DSQL** and authenticated via **AWS Cognito User Pools**.

---

## 1. Architectural Highlights

### Amazon Aurora DSQL Integration ([ADR-003](./adr/ADR-003-aws-dsql-database.md))
- **Endpoint Auto-Detection**: Luminair automatically detects Aurora DSQL from the `*.dsql.<region>.on.aws` hostname or `?dsql=true` query parameter, activating client-side UUIDv7 key generation, standalone non-transactional DDL migrations (`-- no-transaction`), and automatic OCC retry loops on SQLSTATE `40001` serialization conflicts.
- **IAM Authentication & Dynamic Refresh**: Aurora DSQL authenticates via short-lived AWS IAM tokens valid for 15 minutes. The dynamic token provider (`AwsSdkDsqlTokenProvider`) automatically regenerates tokens, while SQLx pool recycling (`max_lifetime = 10m`, `idle_timeout = 5m`) guarantees connections close well before tokens expire.
- **Serverless Connection Sizing**: In AWS Lambda environments, connection pool capacity is clamped to `max_connections = 1` (clamped to max 2) to prevent distributed connection pool exhaustion across auto-scaling sandboxes. For container microservices, `max_connections` defaults to 20–50.

### AWS Cognito Authentication
- **Dual Authentication Paradigms**:
  1. **Serverless Edge Authentication (`aws-lambda` + `luminair-auth-cognito-edge`)**: Amazon API Gateway HTTP API (v2) validates Cognito JWTs directly at the AWS network edge, parameter-mapping verified claims (`sub`, `email`, `name`) into HTTP headers. Unauthenticated requests are rejected at the edge with **$0 Lambda invocation cost**, and the Lambda binary executes with **zero in-process RSA cryptography**.
  2. **Microservice In-Process Authentication (`luminair-server` + `luminair-auth`)**: The long-running daemon validates incoming Bearer JWTs in-process against the Cognito JWKS endpoint or configured RSA public key PEM, and resolves machine-to-machine service tokens.

### Deterministic Pre-Deployment Migrations (`luminair-cli`)
- **Never Run Migrations on Cold Start**: Running dynamic DDL migrations during Lambda cold starts causes distributed OCC write contention and cold-start latency spikes.
- **Pre-Deployment Pipeline**: Run migrations as an explicit, isolated CI/CD or deployment pipeline step prior to shifting traffic:
  ```bash
  luminair-cli --mode validate --schema-dir ./schemas
  luminair-cli --mode dry-run --database-url "$DATABASE_URL" --schema-dir ./schemas
  luminair-cli --mode migrate --database-url "$DATABASE_URL" --schema-dir ./schemas
  ```

---

## 2. Target A: Serverless AWS Lambda Deployment

Deploying Luminair as a serverless function on AWS Lambda provides near-zero idle cost, instant autoscaling, and Graviton2/3 price-performance.

```
                  ┌───────────────────────────────────────────────┐
                  │          AWS Network Edge (API Gateway)       │
                  │                                               │
Client Request ──►│ 1. Verify Cognito JWT (RS256, iss, aud, exp)   │
                  │    ├─ Invalid ──► 401 Unauthorized ($0 cost)  │
                  │    └─ Valid   ──► Parameter Mappings          │
                  │                   • x-cognito-sub             │
                  │                   • x-cognito-email           │
                  │                   • x-cognito-name            │
                  └───────────────────────┬───────────────────────┘
                                          │
                                          │ Invocations with verified headers
                                          ▼
                  ┌───────────────────────────────────────────────┐
                  │         AWS Lambda Sandbox (arm64)            │
                  │                                               │
                  │  bootstrap (aws-lambda binary)                │
                  │  ├─ CognitoEdgeAuthResolver (Zero Crypto)     │
                  │  ├─ PgPool (max_connections = 1)              │
                  │  └─ Bundled Schemas (/var/task/schemas)       │
                  └───────────────────────┬───────────────────────┘
                                          │
                                          │ IAM SigV4 SSL
                                          ▼
                  ┌───────────────────────────────────────────────┐
                  │             Amazon Aurora DSQL                │
                  │         (*.dsql.[region].on.aws)              │
                  └───────────────────────────────────────────────┘
```

### 2.1 Building and Packaging with `cargo-lambda`

Luminair compiles into a native `bootstrap` executable for the `provided.al2023` runtime targeting `arm64` (AWS Graviton):

```bash
# 1. Install cargo-lambda if not already installed
cargo install cargo-lambda

# 2. Build release binary for arm64
cargo lambda build -p aws-lambda --release --arm64

# 3. Package the binary along with the project's bundled schemas
mkdir -p package/schemas
cp -r ./config/schemas/* package/schemas/
cp target/lambda/aws-lambda/bootstrap package/bootstrap

# 4. Create the deployment ZIP archive
cd package && zip -r ../luminair-lambda.zip bootstrap schemas/ && cd ..
```

### 2.2 Bundled Schemas Strategy
Luminair is project-agnostic: any application embeds its declarative JSON schemas (`system-config.json` and `document-types/*.json`) directly inside the Lambda deployment artifact.
Set the environment variable:
```dotenv
LUMINAIR_SCHEMA_DIR=/var/task/schemas
```
During Lambda INIT phase (cold start), `load_system_context_from_dir` reads and caches the schemas in zero-copy `&'static SystemContext`.

### 2.3 Lambda Environment Variables

| Variable | Recommended Value | Description |
|---|---|---|
| `LUMINAIR_DATABASE_URL` | `postgres://admin@<id>.dsql.<region>.on.aws:5432/postgres?sslmode=require` | Aurora DSQL connection endpoint |
| `LUMINAIR_SCHEMA_DIR` | `/var/task/schemas` | Location of bundled declarative JSON schemas |
| `LUMINAIR_MAX_DB_CONNECTIONS` | `1` | Automatically clamped to `1..2` in Lambda sandboxes |
| `LUMINAIR_SAFETY_POLICY` | `additive` | Prevent destructive schema changes in production |
| `RUST_LOG` | `info,luminair_http=warn` | Structured logging configuration |

### 2.4 AWS SAM / CloudFormation Template Example

The following AWS SAM template configures Amazon API Gateway HTTP API (v2) with a native Cognito JWT Authorizer and Parameter Mappings:

```yaml
AWSTemplateFormatVersion: '2010-09-09'
Transform: AWS::Serverless-2016-10-31
Description: Luminair CMS Serverless Deployment on AWS Lambda with Edge Cognito Auth

Parameters:
  CognitoUserPoolArn:
    Type: String
    Description: ARN of the AWS Cognito User Pool
  CognitoUserPoolClientId:
    Type: String
    Description: Cognito App Client ID for Luminair
  CognitoUserPoolIssuer:
    Type: String
    Description: Cognito User Pool Issuer URL (e.g. https://cognito-idp.us-east-1.amazonaws.com/us-east-1_Example)
  DsqlEndpoint:
    Type: String
    Description: Aurora DSQL cluster endpoint (e.g. a1b2c3d4e5f6g7h8.dsql.us-east-1.on.aws)

Resources:
  # ---------------------------------------------------------------------------
  # Amazon API Gateway HTTP API (v2)
  # ---------------------------------------------------------------------------
  LuminairHttpApi:
    Type: AWS::Serverless::HttpApi
    Properties:
      CorsConfiguration:
        AllowOrigins:
          - "https://admin.example.com"
        AllowHeaders:
          - "Authorization"
          - "Content-Type"
          - "x-request-id"
        AllowMethods:
          - GET
          - POST
          - PUT
          - PATCH
          - DELETE
          - OPTIONS
        MaxAge: 3600
      Auth:
        DefaultAuthorizer: CognitoAuthorizer
        Authorizers:
          CognitoAuthorizer:
            Identity:
              Header: Authorization
            JwtConfiguration:
              issuer: !Ref CognitoUserPoolIssuer
              audience:
                - !Ref CognitoUserPoolClientId

  # ---------------------------------------------------------------------------
  # AWS Lambda Function (arm64 Graviton)
  # ---------------------------------------------------------------------------
  LuminairFunction:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: luminair-backend-lambda
      PackageType: Zip
      CodeUri: ./luminair-lambda.zip
      Handler: bootstrap
      Runtime: provided.al2023
      Architectures:
        - arm64
      MemorySize: 512
      Timeout: 15
      Environment:
        Variables:
          LUMINAIR_SCHEMA_DIR: /var/task/schemas
          LUMINAIR_DATABASE_URL: !Sub "postgres://admin@${DsqlEndpoint}:5432/postgres?sslmode=require"
          LUMINAIR_MAX_DB_CONNECTIONS: "1"
          LUMINAIR_SAFETY_POLICY: additive
          RUST_LOG: "info"
      Policies:
        # Permission to connect to Aurora DSQL via IAM authentication
        - Statement:
            - Effect: Allow
              Action:
                - dsql:DbConnect
                - dsql:DbConnectAdmin
              Resource: !Sub "arn:aws:dsql:${AWS::Region}:${AWS::AccountId}:cluster/*"
      Events:
        # Proxy all routes through API Gateway v2
        ProxyApi:
          Type: HttpApi
          Properties:
            ApiId: !Ref LuminairHttpApi
            Path: /{proxy+}
            Method: ANY
            # Edge Parameter Mappings: API Gateway overwrites headers with verified token claims
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

---

## 3. Target B: Container Microservice Deployment (ECS Fargate / EC2)

For persistent containerized environments (Amazon ECS Fargate, EKS, Docker Compose, or Kubernetes), run the long-running daemon `luminair-server`.

### 3.1 Container Environment Configuration

```dotenv
# ==============================================================================
# Luminair Microservice Production Environment (AWS ECS Fargate / Docker)
# ==============================================================================

# Execution & Schemas
LUMINAIR_SCHEMA_DIR=/app/config/schemas
LUMINAIR_AUTO_MIGRATE=false

# HTTP Server Configuration (Binds to 0.0.0.0 inside container)
LUMINAIR_HTTP_HOST=0.0.0.0
LUMINAIR_HTTP_PORT=8080

# Database: Amazon Aurora DSQL
# Hostname automatically configures Aurora DSQL pool options (SSL required)
LUMINAIR_DATABASE_URL=postgres://admin@a1b2c3d4e5f6g7h8.dsql.us-east-1.on.aws:5432/postgres?sslmode=require
LUMINAIR_MAX_DB_CONNECTIONS=20
LUMINAIR_SAFETY_POLICY=additive

# Authentication: AWS Cognito User Pool (In-Process RS256 Verification)
LUMINAIR_OIDC_ISSUER=https://cognito-idp.us-east-1.amazonaws.com/us-east-1_ExamplePoolId
LUMINAIR_OIDC_AUDIENCE=4example7app9client1id234567

# Optional: Static RS256 RSA public key PEM extracted from Cognito JWKS:
LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM="-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA0exampleKey1234567890...\n-----END PUBLIC KEY-----"

# Machine-to-Machine Service API Tokens
# Format: <token>:<service_id>:<role1>,<role2>;...
LUMINAIR_SERVICE_TOKENS="prod-sec-token-9f8a:cloudwatch-monitor:admin;prod-sec-token-3b1c:ingestion-pipeline:editor"

# Observability
RUST_LOG=info,luminair_http=warn
```

### 3.2 Building Multi-Stage Docker Image

The workspace `Dockerfile` compiles both `luminair-server` and `luminair-cli`:

```dockerfile
FROM rust:1.82-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p luminair-server -p luminair-cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/luminair-server /usr/local/bin/luminair-server
COPY --from=builder /app/target/release/luminair-cli /usr/local/bin/luminair-cli
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/luminair-server"]
```

---

## 4. Pre-Deployment Database Migrations Pipeline

In both container and serverless deployments, avoid automatic migrations during application runtime. Use `luminair-cli` in your deployment automation pipeline (e.g., GitHub Actions, AWS CodeBuild, or an ECS RunTask one-shot job):

```bash
# Step 1: Validate schemas offline
luminair-cli --mode validate --schema-dir ./schemas

# Step 2: Inspect planned SQL changes (dry run)
luminair-cli --mode dry-run \
  --schema-dir ./schemas \
  --database-url "$LUMINAIR_DATABASE_URL" \
  --safety-policy additive

# Step 3: Apply migrations deterministically before shifting traffic
luminair-cli --mode migrate \
  --schema-dir ./schemas \
  --database-url "$LUMINAIR_DATABASE_URL" \
  --safety-policy additive
```

---

## 5. Security & IAM Permissions

### 5.1 Aurora DSQL IAM Authentication Policy
Whether running on ECS or Lambda, the execution role requires permission to connect to the Aurora DSQL cluster:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "dsql:DbConnect",
        "dsql:DbConnectAdmin"
      ],
      "Resource": "arn:aws:dsql:us-east-1:123456789012:cluster/a1b2c3d4e5f6g7h8"
    }
  ]
}
```

### 5.2 Spoofing Protection at API Gateway
Because parameter mappings overwrite headers:
- Any incoming `x-cognito-sub`, `x-cognito-email`, or `x-cognito-name` header supplied by an untrusted client is completely overwritten with the verified token claims.
- The `CognitoEdgeAuthResolver` trusts these headers only because they are anchored by API Gateway's cryptographically verified edge token authorizer.
- Never expose the Lambda function via an unauthenticated Function URL; always route traffic through API Gateway HTTP API (v2) with parameter mappings enabled.
