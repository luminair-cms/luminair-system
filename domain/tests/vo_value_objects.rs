//! Spec: docs/specs/domain/value-objects.md — generated from behavior specification rules.

use domain::schema::{AttributeId, DocumentTypeId, RegexPattern};
use domain::system::LocaleId;
use proptest::prelude::*;

proptest! {
    #[test]
    fn vo_01_attribute_id_valid_kebab_case(s in "[a-z][a-z0-9]{1,3}(-[a-z0-9]{1,4}){0,3}") {
        prop_assume!(s.chars().count() >= 2 && s.chars().count() <= 64);
        let res = AttributeId::try_new(&s);
        prop_assert!(res.is_ok(), "Expected valid AttributeId for '{}', got {:?}", s, res);
        let val = res.unwrap();
        prop_assert_eq!(val.as_ref(), s.as_str());
    }

    #[test]
    fn vo_02_document_type_id_valid_kebab_case(s in "[a-z][a-z0-9]{1,3}(-[a-z0-9]{1,4}){0,3}") {
        prop_assume!(s.chars().count() >= 2 && s.chars().count() <= 64);
        let res = DocumentTypeId::try_new(&s);
        prop_assert!(res.is_ok(), "Expected valid DocumentTypeId for '{}', got {:?}", s, res);
        let val = res.unwrap();
        prop_assert_eq!(val.as_ref(), s.as_str());
    }

    #[test]
    fn vo_03_locale_id_conforms_to_bcp47(
        lang in "[a-z]{2,3}",
        region in proptest::option::of("-[A-Z]{2}")
    ) {
        let code = format!("{}{}", lang, region.unwrap_or_default());
        prop_assume!(code.chars().count() <= 16);
        let res = LocaleId::try_new(&code);
        prop_assert!(res.is_ok(), "Expected valid LocaleId for '{}', got {:?}", code, res);
        let val = res.unwrap();
        prop_assert_eq!(val.as_ref(), code.as_str());
    }

    #[test]
    fn vo_04_regex_pattern_compilation_and_matching(
        literal in "[a-zA-Z0-9]{1,10}"
    ) {
        let pattern_str = format!("^{}$", literal);
        let res = RegexPattern::try_new(&pattern_str);
        prop_assert!(res.is_ok(), "pattern compilation failed");
        let rp = res.unwrap();
        prop_assert!(rp.is_match(&literal), "literal did not match");
        let non_match = format!("{}x", literal);
        prop_assert!(!rp.is_match(&non_match), "unexpected match for non_match");
    }
}

#[test]
fn vo_01_attribute_id_boundary_examples() {
    assert!(AttributeId::try_new("ab").is_ok());
    assert!(AttributeId::try_new("title").is_ok());
    assert!(AttributeId::try_new("user-id").is_ok());
    assert!(AttributeId::try_new("meta-tag-123").is_ok());

    assert!(AttributeId::try_new("a").is_err()); // too short
    assert!(AttributeId::try_new("A-b").is_err()); // uppercase
    assert!(AttributeId::try_new("a_b").is_err()); // underscore
    assert!(AttributeId::try_new("-ab").is_err()); // leading dash
    assert!(AttributeId::try_new("ab-").is_err()); // trailing dash
    assert!(AttributeId::try_new("").is_err()); // empty
}

#[test]
fn vo_02_document_type_id_boundary_examples() {
    assert!(DocumentTypeId::try_new("article").is_ok());
    assert!(DocumentTypeId::try_new("blog-post").is_ok());
    assert!(DocumentTypeId::try_new("page-v2").is_ok());

    assert!(DocumentTypeId::try_new("a").is_err());
    assert!(DocumentTypeId::try_new("Blog").is_err());
    assert!(DocumentTypeId::try_new("blog__post").is_err());
    assert!(DocumentTypeId::try_new("blog--post").is_err());
}

#[test]
fn vo_03_locale_id_boundary_examples() {
    assert!(LocaleId::try_new("en").is_ok());
    assert!(LocaleId::try_new("uk").is_ok());
    assert!(LocaleId::try_new("fr").is_ok());
    assert!(LocaleId::try_new("es-ES").is_ok());
    assert!(LocaleId::try_new("en-US").is_ok());
    assert!(LocaleId::try_new("de-DE").is_ok());
    assert!(LocaleId::try_new("ast").is_ok());

    assert!(LocaleId::try_new("EN").is_err());
    assert!(LocaleId::try_new("en_US").is_err());
    assert!(LocaleId::try_new("en-us").is_err());
    assert!(LocaleId::try_new("english").is_err());
    assert!(LocaleId::try_new("").is_err());
    assert!(LocaleId::try_new("e").is_err());
}

#[test]
fn vo_04_regex_pattern_boundary_examples() {
    assert!(RegexPattern::try_new("^[a-z]+$").is_ok());
    assert!(RegexPattern::try_new("\\d{3}-\\d{2}").is_ok());

    assert!(RegexPattern::try_new("[a-z").is_err());
    assert!(RegexPattern::try_new("(").is_err());
    assert!(RegexPattern::try_new("*").is_err());
}
