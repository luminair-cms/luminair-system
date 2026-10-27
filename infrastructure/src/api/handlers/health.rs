//! Health probe route handlers for system liveness and readiness monitoring.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::api::state::HasHealthChecker;
use crate::health::{HealthCheckService, HealthStatus};

// ---------------------------------------------------------------------------
// Public Handler Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Health probe handler verifying basic application liveness (`GET /healthz`).
pub async fn healthz<St: HasHealthChecker>(State(state): State<St>) -> Response {
    let status = state.health_checker().liveness().await;
    health_to_response(status)
}

/// Readiness probe handler verifying system dependencies are reachable (`GET /ready`).
pub async fn ready<St: HasHealthChecker>(State(state): State<St>) -> Response {
    let status = state.health_checker().readiness().await;
    health_to_response(status)
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn health_to_response(status: HealthStatus) -> Response {
    let status_code = if status.is_healthy() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status_code, Json(status)).into_response()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone)]
    struct MockHealthState {
        checker: Arc<MockHealthChecker>,
    }

    impl HasHealthChecker for MockHealthState {
        type Health = MockHealthChecker;

        fn health_checker(&self) -> &Self::Health {
            &self.checker
        }
    }

    struct MockHealthChecker {
        liveness_status: HealthStatus,
        readiness_status: HealthStatus,
    }

    impl HealthCheckService for MockHealthChecker {
        async fn liveness(&self) -> HealthStatus {
            self.liveness_status.clone()
        }

        async fn readiness(&self) -> HealthStatus {
            self.readiness_status.clone()
        }
    }

    #[tokio::test]
    async fn test_healthz_healthy() {
        let state = MockHealthState {
            checker: Arc::new(MockHealthChecker {
                liveness_status: HealthStatus::Healthy,
                readiness_status: HealthStatus::Healthy,
            }),
        };

        let response = healthz(State(state)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ready_unhealthy() {
        let state = MockHealthState {
            checker: Arc::new(MockHealthChecker {
                liveness_status: HealthStatus::Healthy,
                readiness_status: HealthStatus::Unhealthy {
                    reason: "db pool exhausted".to_string(),
                },
            }),
        };

        let response = ready(State(state)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
