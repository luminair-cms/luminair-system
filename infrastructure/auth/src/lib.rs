//! Authentication and authorization infrastructure adapter.
//!
//! Provides OIDC JWT validation, service token authentication, and caller identity resolution.

pub mod claims;
pub mod error;
pub mod oidc;
pub mod resolver;
pub mod service_token;

pub use claims::AuthenticatedUser;
pub use error::{AuthError, AuthResolutionError};
pub use luminair_http::auth::{AuthContextResolver, AuthCredentials};
pub use oidc::{JwtClaims, OidcTokenValidator};
pub use resolver::{DefaultAuthResolver, SqlxAuthResolver};
pub use service_token::{ServiceTokenEntry, ServiceTokenRegistry};
