# About

Luminair is a schema-driven CMS backend written in Rust, exposing a Strapi-like REST API.

## Coding

Main code is written by hand with help of AI (review, analysis, code snippets etc).

Documentation and Tests are written mostly by AI.

## Deployment

**Primary target**: AWS (Aurora DSQL, ECS / Lambda, IAM auth).

**Also works** in any local, Docker, or Kubernetes environment with a standard PostgreSQL database —
no AWS-specific code leaks into the `domain` or `application` layers.
AWS-specific adapters (DSQL IAM token refresh, etc.) are isolated in the `infrastructure` crate
and are swappable without touching business logic.