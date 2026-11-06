//! Authentication and authorization machinery.

mod claims;
mod error;
mod extractor;
mod oidc;
mod resolver;
mod service_token;

pub use oidc::OidcTokenValidator;
pub use resolver::DefaultAuthResolver;
pub use service_token::ServiceTokenRegistry;

#[cfg(test)]
pub(crate) use claims::*;
#[cfg(test)]
pub(crate) use error::*;
pub(crate) use extractor::*;
pub(crate) use resolver::*;
