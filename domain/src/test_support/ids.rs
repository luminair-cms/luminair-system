use crate::auth::user::{ActorId, ServiceId, UserId};
use crate::schema::attributes::AttributeId;
use crate::schema::documents::DocumentTypeId;
use crate::system::locale::LocaleId;

/// Shorthand to construct a valid `ActorId::User` for tests.
pub fn create_actor_user(name: &str) -> ActorId {
    ActorId::User(create_user_id(name))
}

/// Shorthand to construct a valid `UserId` for tests.
pub fn create_user_id(name: &str) -> UserId {
    UserId::try_new(name).expect("valid user id")
}

/// Shorthand to construct a valid `ServiceId` for tests.
pub fn create_service_id(name: &str) -> ServiceId {
    ServiceId::try_new(name).expect("valid service id")
}

/// Shorthand to construct a valid kebab-case `DocumentTypeId` for tests.
pub fn create_doc_type_id(name: &str) -> DocumentTypeId {
    DocumentTypeId::try_new(name).expect("valid doc type id")
}

/// Shorthand to construct a valid kebab-case `AttributeId` for tests.
pub fn create_attr_id(name: &str) -> AttributeId {
    AttributeId::try_new(name).expect("valid attribute id")
}

/// Returns standard test locales: ("en", "uk", "fr").
pub fn create_locales() -> (LocaleId, LocaleId, LocaleId) {
    (
        create_locale_id("en"),
        create_locale_id("uk"),
        create_locale_id("fr"),
    )
}

/// Shorthand to construct a valid `LocaleId` for tests.
pub fn create_locale_id(code: &str) -> LocaleId {
    LocaleId::try_new(code).expect("valid locale id")
}

// Backward-compatibility aliases
pub use create_actor_user as test_actor_user;
pub use create_attr_id as test_attr_id;
pub use create_doc_type_id as test_doc_type_id;
pub use create_locale_id as test_locale_id;
pub use create_locales as test_locales;
pub use create_service_id as test_service_id;
pub use create_user_id as test_user_id;
