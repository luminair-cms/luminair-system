//! SQLx repository adapters, row mappers, JSONB/URN codecs, and relation helpers.

pub mod access_request;
pub mod codec;
pub mod document_instance;
pub mod document_snapshot;
pub mod mappers;
pub mod relations;
pub mod role;
pub mod user_role_assignment;

pub use access_request::SqlxAccessRequestRepository;
pub use document_instance::SqlxDocumentInstanceRepository;
pub use document_snapshot::SqlxDocumentSnapshotRepository;
pub use role::SqlxRoleRepository;
pub use user_role_assignment::SqlxUserRoleAssignmentRepository;
