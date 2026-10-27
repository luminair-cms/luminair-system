//! HTTP REST driving adapter, generic state, error mapping, and codecs.

pub mod codec;
pub mod envelope;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod state;

pub use envelope::{CreatedJson, JsonData, PagedJson, PaginationMeta};
pub use error::{ApiError, ApiErrorKind, ApiProblemDetails};
pub use extractors::*;
pub use routes::build_router;
pub use state::*;
