//! Generic REST API route handlers for content, schemas, access requests, and system probes.

pub mod access_requests;
pub mod content;
pub mod health;
pub mod schema;
pub mod system;

pub use access_requests::*;
pub use content::*;
pub use health::*;
pub use schema::*;
pub use system::*;
