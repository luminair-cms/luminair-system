use std::collections::HashSet;

use crate::content::DocumentContent;
use crate::content::values::{ContentValue, DomainValue, PrimitiveValue};
use crate::errors::DomainError;
use crate::schema::types::FieldType;
use crate::schema::{AttributeId, DocumentType, FieldConstraint};
use crate::system::LocaleId;
use crate::system::config::SystemConfig;

/// Validates document instance content for draft saving (lenient: skips required checks for missing/null).
///
/// # Errors
/// Returns a collection of [`DomainError`] if any field value violates constraint or type rules.
pub fn validate_draft_content(
    doc_type: &DocumentType,
    content: &DocumentContent,
    system_config: &SystemConfig,
) -> Result<(), Vec<DomainError>> {
    validate_content_with_mode(doc_type, content, system_config, false)
}

/// Validates document instance content against strict schema rules for publication.
///
/// # Errors
/// Returns a collection of [`DomainError`] if required fields are missing or constraint/type rules fail.
pub fn validate_publish_content(
    doc_type: &DocumentType,
    content: &DocumentContent,
    system_config: &SystemConfig,
) -> Result<(), Vec<DomainError>> {
    validate_content_with_mode(doc_type, content, system_config, true)
}

fn validate_content_with_mode(
    doc_type: &DocumentType,
    content: &DocumentContent,
    system_config: &SystemConfig,
    strict: bool,
) -> Result<(), Vec<DomainError>> {
    let mut errors = Vec::new();

    // 1. Validate declared fields
    for field_def in &doc_type.fields {
        let attr_id = &field_def.id;
        match content.get_field(attr_id) {
            None | Some(ContentValue::Null) => {
                if strict && field_def.required {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: "field is required but missing or null".to_string(),
                    });
                }
            }
            Some(val) => {
                validate_field_value(
                    attr_id,
                    field_def.field_type,
                    &field_def.constraints,
                    val,
                    system_config,
                    &mut errors,
                );
            }
        }
    }

    // 2. Reject undeclared fields
    for attr_id in content.fields().keys() {
        if !doc_type.fields.contains(attr_id) {
            errors.push(DomainError::UnknownAttribute(attr_id.clone()));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::mutable_key_type)]
fn validate_field_value(
    attr_id: &AttributeId,
    field_type: FieldType,
    constraints: &HashSet<FieldConstraint>,
    val: &ContentValue,
    system_config: &SystemConfig,
    errors: &mut Vec<DomainError>,
) {
    match val {
        ContentValue::Null => {}
        ContentValue::Scalar(dv) => {
            if dv.matches_field_type(&field_type) {
                for constraint in constraints {
                    if let Some(err) = evaluate_constraint(attr_id, constraint, dv) {
                        errors.push(err);
                    }
                }
            } else {
                errors.push(DomainError::InvalidFieldValue {
                    attribute_id: attr_id.clone(),
                    reason: format!("value does not match field type {field_type:?}"),
                });
            }
        }
        ContentValue::LocalizedText(map) => {
            if field_type == FieldType::LocalizedText {
                let mut sorted_locales: Vec<_> = map.keys().collect();
                sorted_locales.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
                for locale in sorted_locales {
                    let localized_str = &map[locale];
                    if system_config.contains_locale(locale) {
                        for constraint in constraints {
                            if let Some(err) = evaluate_str_constraint(
                                attr_id,
                                constraint,
                                localized_str,
                                Some(locale),
                            ) {
                                errors.push(err);
                            }
                        }
                    } else {
                        errors.push(DomainError::UnknownLocale(locale.clone()));
                    }
                }
            } else {
                errors.push(DomainError::InvalidFieldValue {
                    attribute_id: attr_id.clone(),
                    reason: "localized text value provided for non-localized field".to_string(),
                });
            }
        }
    }
}

/// Evaluates a single `FieldConstraint` against a scalar `DomainValue`.
/// Returns `Some(DomainError)` if the constraint is violated, `None` if satisfied.
fn evaluate_constraint(
    attr_id: &AttributeId,
    constraint: &FieldConstraint,
    value: &DomainValue,
) -> Option<DomainError> {
    let violated = |reason: String| -> Option<DomainError> {
        Some(DomainError::InvalidFieldValue {
            attribute_id: attr_id.clone(),
            reason,
        })
    };

    match (constraint, value) {
        (
            FieldConstraint::MinLength(_)
            | FieldConstraint::MaxLength(_)
            | FieldConstraint::Pattern(_),
            DomainValue::Primitive(PrimitiveValue::Text(s) | PrimitiveValue::Uid(s)),
        ) => evaluate_str_constraint(attr_id, constraint, s, None),
        (FieldConstraint::MinInteger(min), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n < *min {
                return violated(format!("value {n} is below minimum {min}"));
            }
            None
        }
        (FieldConstraint::MaxInteger(max), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n > *max {
                return violated(format!("value {n} exceeds maximum {max}"));
            }
            None
        }
        (FieldConstraint::MinDecimal(min), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d < min {
                return violated(format!("value {d} is below minimum {min}"));
            }
            None
        }
        (FieldConstraint::MaxDecimal(max), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d > max {
                return violated(format!("value {d} exceeds maximum {max}"));
            }
            None
        }
        // Constraint not applicable to this value type — silently skip (type validation catches mismatches)
        _ => None,
    }
}

/// Evaluates string constraints (`MinLength`, `MaxLength`, `Pattern`) against a string value.
/// If `locale` is provided, includes it in the error reason.
fn evaluate_str_constraint(
    attr_id: &AttributeId,
    constraint: &FieldConstraint,
    s: &str,
    locale: Option<&LocaleId>,
) -> Option<DomainError> {
    let prefix = match locale {
        Some(loc) => format!("locale '{}': ", loc.as_ref()),
        None => String::new(),
    };
    let violated = |reason: String| -> Option<DomainError> {
        Some(DomainError::InvalidFieldValue {
            attribute_id: attr_id.clone(),
            reason: format!("{prefix}{reason}"),
        })
    };

    match constraint {
        FieldConstraint::MinLength(min) => {
            let len = s.chars().count();
            if len < *min {
                return violated(format!("length {len} is below minimum {min}"));
            }
        }
        FieldConstraint::MaxLength(max) => {
            let len = s.chars().count();
            if len > *max {
                return violated(format!("length {len} exceeds maximum {max}"));
            }
        }
        FieldConstraint::Pattern(pattern) if !pattern.is_match(s) => {
            return violated(format!(
                "value '{s}' does not match pattern '{}'",
                pattern.as_str()
            ));
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::collections::HashMap;

    use crate::schema::pattern::RegexPattern;
    use crate::schema::types::IntegerSize;
    use crate::test_support::{
        DocumentTypeBuilder, create_attr_id, create_locales, decimal_field, draft_content,
        fixture_system_config, integer_field, localized_field, text_field,
    };

    fn make_test_setup() -> (DocumentType, SystemConfig, AttributeId) {
        let title_attr = create_attr_id("title");
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title").required(true))
            .build();
        (doc_type, fixture_system_config(), title_attr)
    }

    #[test]
    fn test_validate_content_required_field_missing_returns_error() {
        let (doc_type, config, title_attr) = make_test_setup();
        let content = draft_content(HashMap::new());

        let result = validate_publish_content(&doc_type, &content, &config);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DomainError::InvalidFieldValue { attribute_id, reason }
                if *attribute_id == title_attr && reason.contains("required")
        ));
    }

    #[test]
    fn test_validate_content_required_field_null_returns_error() {
        let (doc_type, config, title_attr) = make_test_setup();
        let mut fields = HashMap::new();
        fields.insert(title_attr.clone(), ContentValue::Null);
        let content = draft_content(fields);

        let result = validate_publish_content(&doc_type, &content, &config);
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DomainError::InvalidFieldValue { attribute_id, reason }
                if *attribute_id == title_attr && reason.contains("required")
        ));
    }

    #[test]
    fn test_validate_content_optional_field_absent_or_null_passes() {
        let title_attr = create_attr_id("title");
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title").required(false))
            .build();
        let config = fixture_system_config();

        // Case 1: absent
        let empty_content = draft_content(HashMap::new());
        assert!(validate_publish_content(&doc_type, &empty_content, &config).is_ok());

        // Case 2: Null
        let mut fields = HashMap::new();
        fields.insert(title_attr, ContentValue::Null);
        let null_content = draft_content(fields);
        assert!(validate_publish_content(&doc_type, &null_content, &config).is_ok());
    }

    #[test]
    fn test_validate_content_type_mismatch_returns_error() {
        let (doc_type, config, title_attr) = make_test_setup();
        let mut fields = HashMap::new();
        // Provide integer for text field
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(42))),
        );
        let content = draft_content(fields);

        let errors = validate_publish_content(&doc_type, &content, &config).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DomainError::InvalidFieldValue { attribute_id, reason }
                if *attribute_id == title_attr && reason.contains("does not match field type")
        ));
    }

    #[test]
    fn test_validate_content_string_length_and_pattern_constraints() {
        let title_attr = create_attr_id("title");
        let pattern = RegexPattern::try_new(r"^[A-Z][a-z]+$").unwrap();
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title").min_len(3).max_len(10).pattern(pattern))
            .build();
        let config = fixture_system_config();

        // Valid: capitalized, len between 3 and 10
        let mut fields = HashMap::new();
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Hello".into()))),
        );
        assert!(validate_publish_content(&doc_type, &draft_content(fields), &config).is_ok());

        // Invalid: too short
        let mut short_fields = HashMap::new();
        short_fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Hi".into()))),
        );
        let short_err =
            validate_publish_content(&doc_type, &draft_content(short_fields), &config).unwrap_err();
        assert!(short_err.iter().any(|e| matches!(e, DomainError::InvalidFieldValue { reason, .. } if reason.contains("below minimum"))));

        // Invalid: pattern mismatch
        let mut regex_fields = HashMap::new();
        regex_fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("hello".into()))),
        );
        let pattern_err =
            validate_publish_content(&doc_type, &draft_content(regex_fields), &config).unwrap_err();
        assert!(pattern_err.iter().any(|e| matches!(e, DomainError::InvalidFieldValue { reason, .. } if reason.contains("does not match pattern"))));
    }

    #[test]
    fn test_validate_content_numeric_constraints() {
        let count_attr = create_attr_id("count");
        let price_attr = create_attr_id("price");

        let doc_type = DocumentTypeBuilder::collection("product")
            .with_field(
                integer_field("count", IntegerSize::I32)
                    .min_int(1)
                    .max_int(100),
            )
            .with_field(
                decimal_field("price", 6, 2)
                    .min_dec(Decimal::new(1000, 2)) // 10.00
                    .max_dec(Decimal::new(5000, 2)), // 50.00
            )
            .build();
        let config = fixture_system_config();

        // Valid values
        let mut valid_fields = HashMap::new();
        valid_fields.insert(
            count_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(50))),
        );
        valid_fields.insert(
            price_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(
                Decimal::new(2500, 2),
            ))),
        );
        assert!(validate_publish_content(&doc_type, &draft_content(valid_fields), &config).is_ok());

        // Out of range values
        let mut invalid_fields = HashMap::new();
        invalid_fields.insert(
            count_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(150))),
        );
        invalid_fields.insert(
            price_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(
                Decimal::new(500, 2),
            ))), // 5.00
        );
        let errors =
            validate_publish_content(&doc_type, &draft_content(invalid_fields), &config).unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_validate_content_localized_text_validation() {
        let desc_attr = create_attr_id("description");
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(localized_field("description").min_len(4))
            .build();
        let config = fixture_system_config();
        let (en, uk, de) = create_locales(); // de is not in fixture_system_config (which has en, uk)

        // Valid localized text
        let mut valid_locales = HashMap::new();
        valid_locales.insert(en.clone(), "Hello World".to_string());
        valid_locales.insert(uk.clone(), "Привіт світ".to_string());
        let mut fields = HashMap::new();
        fields.insert(
            desc_attr.clone(),
            ContentValue::LocalizedText(valid_locales),
        );
        assert!(validate_publish_content(&doc_type, &draft_content(fields), &config).is_ok());

        // Unknown locale error
        let mut with_de = HashMap::new();
        with_de.insert(de.clone(), "Guten Tag".to_string());
        let mut de_fields = HashMap::new();
        de_fields.insert(desc_attr.clone(), ContentValue::LocalizedText(with_de));
        let de_errors =
            validate_publish_content(&doc_type, &draft_content(de_fields), &config).unwrap_err();
        assert!(matches!(&de_errors[0], DomainError::UnknownLocale(loc) if *loc == de));

        // Constraint violation on a specific locale
        let mut short_locale = HashMap::new();
        short_locale.insert(en, "Hey".to_string()); // len 3 < min 4
        let mut short_fields = HashMap::new();
        short_fields.insert(desc_attr, ContentValue::LocalizedText(short_locale));
        let constraint_err =
            validate_publish_content(&doc_type, &draft_content(short_fields), &config).unwrap_err();
        assert!(matches!(
            &constraint_err[0],
            DomainError::InvalidFieldValue { reason, .. } if reason.contains("locale 'en'")
        ));
    }

    #[test]
    fn test_validate_content_rejects_unknown_attribute() {
        let (doc_type, config, title_attr) = make_test_setup();
        let extra_attr = create_attr_id("non-existent");

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Valid Title".into(),
            ))),
        );
        fields.insert(
            extra_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(100))),
        );

        let errors = validate_publish_content(&doc_type, &draft_content(fields), &config).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DomainError::UnknownAttribute(attr) if *attr == extra_attr
        ));
    }

    #[test]
    fn test_two_tier_validation_draft_vs_publish() {
        let (doc_type, config, title_attr) = make_test_setup();
        let (_, _, de) = create_locales();

        // 1. Missing required field: Draft succeeds, Publish fails
        let empty_content = draft_content(HashMap::new());
        assert!(validate_draft_content(&doc_type, &empty_content, &config).is_ok());
        let pub_errs = validate_publish_content(&doc_type, &empty_content, &config).unwrap_err();
        assert_eq!(pub_errs.len(), 1);
        assert!(
            matches!(&pub_errs[0], DomainError::InvalidFieldValue { attribute_id, .. } if *attribute_id == title_attr)
        );

        // 2. Null required field: Draft succeeds, Publish fails
        let mut null_fields = HashMap::new();
        null_fields.insert(title_attr.clone(), ContentValue::Null);
        let null_content = draft_content(null_fields);
        assert!(validate_draft_content(&doc_type, &null_content, &config).is_ok());
        assert!(validate_publish_content(&doc_type, &null_content, &config).is_err());

        // 3. Type mismatch on present field: Both Draft and Publish fail
        let mut invalid_type_fields = HashMap::new();
        invalid_type_fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(123))),
        );
        let invalid_type_content = draft_content(invalid_type_fields);
        assert!(validate_draft_content(&doc_type, &invalid_type_content, &config).is_err());
        assert!(validate_publish_content(&doc_type, &invalid_type_content, &config).is_err());

        // 4. Unknown locale on localized field: Both Draft and Publish fail
        let localized_doc_type = DocumentTypeBuilder::collection("article-loc")
            .with_field(localized_field("summary"))
            .build();
        let mut de_loc_map = HashMap::new();
        de_loc_map.insert(de.clone(), "Hallo".to_string());
        let mut loc_fields = HashMap::new();
        loc_fields.insert(
            create_attr_id("summary"),
            ContentValue::LocalizedText(de_loc_map),
        );
        let loc_content = draft_content(loc_fields);
        assert!(matches!(
            &validate_draft_content(&localized_doc_type, &loc_content, &config).unwrap_err()[0],
            DomainError::UnknownLocale(loc) if *loc == de
        ));

        // 5. Unknown attribute: Both Draft and Publish fail
        let mut unknown_attr_fields = HashMap::new();
        unknown_attr_fields.insert(
            create_attr_id("unregistered-attr"),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("test".into()))),
        );
        let unknown_attr_content = draft_content(unknown_attr_fields);
        assert!(matches!(
            &validate_draft_content(&doc_type, &unknown_attr_content, &config).unwrap_err()[0],
            DomainError::UnknownAttribute(attr) if attr.as_ref() == "unregistered-attr"
        ));

        // 6. Field constraint violation on non-null present field: Both Draft and Publish fail
        let constrained_doc_type = DocumentTypeBuilder::collection("article-constrained")
            .with_field(text_field("title").min_len(5))
            .build();
        let mut short_fields = HashMap::new();
        short_fields.insert(
            create_attr_id("title"),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Hi".into()))),
        );
        let short_content = draft_content(short_fields);
        assert!(validate_draft_content(&constrained_doc_type, &short_content, &config).is_err());
        assert!(validate_publish_content(&constrained_doc_type, &short_content, &config).is_err());
    }
}
