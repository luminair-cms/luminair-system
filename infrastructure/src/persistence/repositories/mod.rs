//! SQLx repository adapters, row mappers, JSONB/URN codecs, and relation helpers.

mod access_request;
pub mod codec;
mod document_instance;
mod document_snapshot;
pub(crate) mod mappers;
pub(crate) mod relations;
mod role;
mod user_role_assignment;

pub use access_request::SqlxAccessRequestRepository;
pub use document_instance::SqlxDocumentInstanceRepository;
pub use document_snapshot::SqlxDocumentSnapshotRepository;
pub use role::SqlxRoleRepository;
pub use user_role_assignment::SqlxUserRoleAssignmentRepository;
