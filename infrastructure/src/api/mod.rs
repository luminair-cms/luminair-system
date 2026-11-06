//! HTTP REST driving adapter, generic state, error mapping, and codecs.

pub(crate) mod codec;
pub(crate) mod envelope;
pub(crate) mod error;
pub(crate) mod extractors;
pub(crate) mod handlers;
pub(crate) mod middleware;
pub mod routes;
pub mod state;
