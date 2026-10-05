pub mod commands;
pub mod context;
pub mod errors;
pub mod services;

/// Fake in-memory repository implementations for use in tests.
/// Only compiled when the `test-support` feature is enabled, or during `cargo test`.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use commands::{
    ApproveAccessRequestCommand, CreateDocumentCommand, DeleteDocumentCommand, FindByIdCommand,
    FindDocumentsCommand, PublishDocumentCommand, RejectAccessRequestCommand, RelationAction,
    SubmitAccessRequestCommand, UnpublishDocumentCommand, UpdateDocumentCommand,
};
pub use context::*;
pub use errors::*;
pub use services::{
    AccessRequestsService, AccessRequestsServiceImpl, DocumentsService, DocumentsServiceImpl,
    SystemConfigService, SystemConfigServiceImpl,
};
