//! HTTP driving adapter, REST endpoints, extractors, and composition root.

pub mod auth;
pub mod codec;
pub mod container;
pub mod envelope;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod state;

pub use auth::{
    AuthContextResolver, AuthCredentials, AuthError, AuthResolutionError, AuthenticatedUser,
};
pub use axum::{self, Router};
pub use container::AppContainer;
pub use error::ApiError;
pub use extractors::{Caller, OnboardingUser};
pub use routes::build_router;
pub use state::HttpState;
