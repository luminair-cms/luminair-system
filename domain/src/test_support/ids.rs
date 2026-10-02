use crate::auth::user::UserId;
use crate::schema::attributes::AttributeId;
use crate::schema::documents::DocumentTypeId;
use crate::system::locale::LocaleId;

/// Shorthand to construct a valid `UserId` for tests.
pub fn test_user_id(name: &str) -> UserId {
    UserId::try_new(name).expect("valid test user id")
}

/// Shorthand to construct a valid kebab-case `DocumentTypeId` for tests.
pub fn test_doc_type_id(name: &str) -> DocumentTypeId {
    DocumentTypeId::try_new(name).expect("valid test doc type id")
}

/// Shorthand to construct a valid kebab-case `AttributeId` for tests.
pub fn test_attr_id(name: &str) -> AttributeId {
    AttributeId::try_new(name).expect("valid test attribute id")
}

/// Shorthand to construct a valid `LocaleId` for tests.
pub fn test_locale_id(code: &str) -> LocaleId {
    LocaleId::try_new(code).expect("valid test locale id")
}

/// Returns standard test locales: ("en", "uk", "fr").
pub fn test_locales() -> (LocaleId, LocaleId, LocaleId) {
    (
        test_locale_id("en"),
        test_locale_id("uk"),
        test_locale_id("fr"),
    )
}
