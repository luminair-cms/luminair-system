//! Authentication and authorization machinery.

pub mod extractor {
    pub use luminair_http::extractors::auth::*;
}

pub use extractor::{Caller, OnboardingUser};
pub use luminair_auth::*;
