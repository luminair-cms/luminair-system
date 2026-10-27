//! Health and readiness check services.

use std::future::Future;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Types & Port Traits
// ---------------------------------------------------------------------------

/// Status reported by liveness and readiness health checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Unhealthy { reason: String },
}

/// Port trait defining health check capabilities for system probes.
pub trait HealthCheckService: Send + Sync + 'static {
    fn liveness(&self) -> impl Future<Output = HealthStatus> + Send;
    fn readiness(&self) -> impl Future<Output = HealthStatus> + Send;
}

/// Database-backed health check service querying the primary PostgreSQL pool.
#[derive(Debug, Clone)]
pub struct SqlxHealthChecker {
    pool: PgPool,
}

/// Zero-dependency health checker that always reports healthy.
///
/// Useful for unit testing and offline development environments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AlwaysHealthyChecker;

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl HealthStatus {
    /// Returns `true` if the status is [`HealthStatus::Healthy`].
    #[must_use]
    pub const fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

impl SqlxHealthChecker {
    /// Creates a new health checker bound to the given connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl HealthCheckService for SqlxHealthChecker {
    async fn liveness(&self) -> HealthStatus {
        HealthStatus::Healthy
    }

    async fn readiness(&self) -> HealthStatus {
        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => HealthStatus::Healthy,
            Err(err) => HealthStatus::Unhealthy {
                reason: err.to_string(),
            },
        }
    }
}

impl HealthCheckService for AlwaysHealthyChecker {
    async fn liveness(&self) -> HealthStatus {
        HealthStatus::Healthy
    }

    async fn readiness(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Unhealthy {
            reason: "connection failed".to_string()
        }
        .is_healthy());
    }

    #[test]
    fn test_health_status_serialization() {
        let healthy = HealthStatus::Healthy;
        let healthy_json = serde_json::to_string(&healthy).expect("serialize healthy");
        assert_eq!(healthy_json, r#"{"status":"healthy"}"#);

        let unhealthy = HealthStatus::Unhealthy {
            reason: "db unreachable".to_string(),
        };
        let unhealthy_json = serde_json::to_string(&unhealthy).expect("serialize unhealthy");
        assert_eq!(
            unhealthy_json,
            r#"{"status":"unhealthy","reason":"db unreachable"}"#
        );

        let roundtrip_healthy: HealthStatus =
            serde_json::from_str(&healthy_json).expect("deserialize healthy");
        assert_eq!(roundtrip_healthy, HealthStatus::Healthy);

        let roundtrip_unhealthy: HealthStatus =
            serde_json::from_str(&unhealthy_json).expect("deserialize unhealthy");
        assert_eq!(roundtrip_unhealthy, unhealthy);
    }

    #[tokio::test]
    async fn test_always_healthy_checker() {
        let checker = AlwaysHealthyChecker;
        assert_eq!(checker.liveness().await, HealthStatus::Healthy);
        assert_eq!(checker.readiness().await, HealthStatus::Healthy);
    }
}
