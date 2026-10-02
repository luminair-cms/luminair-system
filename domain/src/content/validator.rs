use crate::content::DocumentContent;
use crate::content::values::{ContentValue, DomainValue, PrimitiveValue};
use crate::errors::DomainError;
use crate::schema::types::FieldType;
use crate::schema::{AttributeId, DocumentType, FieldConstraint};
use crate::system::LocaleId;
use crate::system::config::SystemConfig;

/// Evaluates string constraints (MinLength, MaxLength, Pattern) against a string value.
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
                return violated(format!("length {} is below minimum {}", len, min));
            }
        }
        FieldConstraint::MaxLength(max) => {
            let len = s.chars().count();
            if len > *max {
                return violated(format!("length {} exceeds maximum {}", len, max));
            }
        }
        FieldConstraint::Pattern(pattern) if !pattern.is_match(s) => {
            return violated(format!(
                "value '{}' does not match pattern '{}'",
                s,
                pattern.as_str()
            ));
        }
        _ => {}
    }
    None
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
            DomainValue::Primitive(PrimitiveValue::Text(s))
            | DomainValue::Primitive(PrimitiveValue::Uid(s)),
        ) => evaluate_str_constraint(attr_id, constraint, s, None),
        (FieldConstraint::MinInteger(min), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n < *min {
                return violated(format!("value {} is below minimum {}", n, min));
            }
            None
        }
        (FieldConstraint::MaxInteger(max), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n > *max {
                return violated(format!("value {} exceeds maximum {}", n, max));
            }
            None
        }
        (FieldConstraint::MinDecimal(min), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d < min {
                return violated(format!("value {} is below minimum {}", d, min));
            }
            None
        }
        (FieldConstraint::MaxDecimal(max), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d > max {
                return violated(format!("value {} exceeds maximum {}", d, max));
            }
            None
        }
        // Constraint not applicable to this value type — silently skip (type validation catches mismatches)
        _ => None,
    }
}

/// Validates document instance content against a `DocumentType` definition and `SystemConfig`.
pub fn validate_content(
    doc_type: &DocumentType,
    content: &DocumentContent,
    system_config: &SystemConfig,
) -> Result<(), Vec<DomainError>> {
    let mut errors = Vec::new();

    // 1. Validate declared fields
    for field_def in &doc_type.fields {
        let attr_id = &field_def.id;
        match content.fields.get(attr_id) {
            None | Some(ContentValue::Null) => {
                if field_def.required {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: "field is required but missing or null".to_string(),
                    });
                }
            }
            Some(ContentValue::Scalar(val)) => {
                if !val.matches_field_type(&field_def.field_type) {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: format!(
                            "value does not match field type {:?}",
                            field_def.field_type
                        ),
                    });
                } else {
                    // Evaluate FieldConstraints against the scalar value
                    for constraint in &field_def.constraints {
                        if let Some(err) = evaluate_constraint(attr_id, constraint, val) {
                            errors.push(err);
                        }
                    }
                }
            }
            Some(ContentValue::LocalizedText(map)) => {
                if field_def.field_type != FieldType::LocalizedText {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: "localized text value provided for non-localized field".to_string(),
                    });
                } else {
                    let mut sorted_locales: Vec<_> = map.keys().collect();
                    sorted_locales.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
                    for locale in sorted_locales {
                        let localized_str = &map[locale];
                        if !system_config.contains_locale(locale) {
                            errors.push(DomainError::UnknownLocale(locale.clone()));
                        } else {
                            for constraint in &field_def.constraints {
                                if let Some(err) = evaluate_str_constraint(
                                    attr_id,
                                    constraint,
                                    localized_str,
                                    Some(locale),
                                ) {
                                    errors.push(err);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Reject undeclared fields
    for attr_id in content.fields.keys() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use rust_decimal::Decimal;

    use crate::schema::pattern::RegexPattern;
    use crate::schema::types::IntegerSize;
    use crate::test_support::{
        decimal_field, draft_content, fixture_system_config, integer_field, localized_field,
        test_attr_id, test_locales, text_field, DocumentTypeBuilder,
    };

    fn make_test_setup() -> (DocumentType, SystemConfig, AttributeId) {
        let title_attr = test_attr_id("title");
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title").required(true))
            .build();
        (doc_type, fixture_system_config(), title_attr)
    }

    #[test]
    fn test_validate_content_required_field_missing_returns_error() {
        let (doc_type, config, title_attr) = make_test_setup();
        let content = draft_content(HashMap::new());

        let result = validate_content(&doc_type, &content, &config);
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

        let result = validate_content(&doc_type, &content, &config);
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
        let title_attr = test_attr_id("title");
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title").required(false))
            .build();
        let config = fixture_system_config();

        // Case 1: absent
        let empty_content = draft_content(HashMap::new());
        assert!(validate_content(&doc_type, &empty_content, &config).is_ok());

        // Case 2: Null
        let mut fields = HashMap::new();
        fields.insert(title_attr, ContentValue::Null);
        let null_content = draft_content(fields);
        assert!(validate_content(&doc_type, &null_content, &config).is_ok());
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

        let errors = validate_content(&doc_type, &content, &config).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DomainError::InvalidFieldValue { attribute_id, reason }
                if *attribute_id == title_attr && reason.contains("does not match field type")
        ));
    }

    #[test]
    fn test_validate_content_string_length_and_pattern_constraints() {
        let title_attr = test_attr_id("title");
        let pattern = RegexPattern::try_new(r"^[A-Z][a-z]+$").unwrap();
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(
                text_field("title")
                    .min_len(3)
                    .max_len(10)
                    .pattern(pattern),
            )
            .build();
        let config = fixture_system_config();

        // Valid: capitalized, len between 3 and 10
        let mut fields = HashMap::new();
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Hello".into()))),
        );
        assert!(validate_content(&doc_type, &draft_content(fields), &config).is_ok());

        // Invalid: too short
        let mut short_fields = HashMap::new();
        short_fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Hi".into()))),
        );
        let short_err = validate_content(&doc_type, &draft_content(short_fields), &config).unwrap_err();
        assert!(short_err.iter().any(|e| matches!(e, DomainError::InvalidFieldValue { reason, .. } if reason.contains("below minimum"))));

        // Invalid: pattern mismatch
        let mut regex_fields = HashMap::new();
        regex_fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("hello".into()))),
        );
        let pattern_err = validate_content(&doc_type, &draft_content(regex_fields), &config).unwrap_err();
        assert!(pattern_err.iter().any(|e| matches!(e, DomainError::InvalidFieldValue { reason, .. } if reason.contains("does not match pattern"))));
    }

    #[test]
    fn test_validate_content_numeric_constraints() {
        let count_attr = test_attr_id("count");
        let price_attr = test_attr_id("price");

        let doc_type = DocumentTypeBuilder::collection("product")
            .with_field(
                integer_field("count", IntegerSize::I32)
                    .min_int(1)
                    .max_int(100),
            )
            .with_field(
                decimal_field("price", 6, 2)
                    .min_dec(Decimal::new(1000, 2))  // 10.00
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
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(Decimal::new(2500, 2)))),
        );
        assert!(validate_content(&doc_type, &draft_content(valid_fields), &config).is_ok());

        // Out of range values
        let mut invalid_fields = HashMap::new();
        invalid_fields.insert(
            count_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(150))),
        );
        invalid_fields.insert(
            price_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(Decimal::new(500, 2)))), // 5.00
        );
        let errors = validate_content(&doc_type, &draft_content(invalid_fields), &config).unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_validate_content_localized_text_validation() {
        let desc_attr = test_attr_id("description");
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(localized_field("description").min_len(4))
            .build();
        let config = fixture_system_config();
        let (en, uk, de) = test_locales(); // de is not in fixture_system_config (which has en, uk)

        // Valid localized text
        let mut valid_locales = HashMap::new();
        valid_locales.insert(en.clone(), "Hello World".to_string());
        valid_locales.insert(uk.clone(), "Привіт світ".to_string());
        let mut fields = HashMap::new();
        fields.insert(desc_attr.clone(), ContentValue::LocalizedText(valid_locales));
        assert!(validate_content(&doc_type, &draft_content(fields), &config).is_ok());

        // Unknown locale error
        let mut with_de = HashMap::new();
        with_de.insert(de.clone(), "Guten Tag".to_string());
        let mut de_fields = HashMap::new();
        de_fields.insert(desc_attr.clone(), ContentValue::LocalizedText(with_de));
        let de_errors = validate_content(&doc_type, &draft_content(de_fields), &config).unwrap_err();
        assert!(matches!(&de_errors[0], DomainError::UnknownLocale(loc) if *loc == de));

        // Constraint violation on a specific locale
        let mut short_locale = HashMap::new();
        short_locale.insert(en, "Hey".to_string()); // len 3 < min 4
        let mut short_fields = HashMap::new();
        short_fields.insert(desc_attr, ContentValue::LocalizedText(short_locale));
        let constraint_err = validate_content(&doc_type, &draft_content(short_fields), &config).unwrap_err();
        assert!(matches!(
            &constraint_err[0],
            DomainError::InvalidFieldValue { reason, .. } if reason.contains("locale 'en'")
        ));
    }

    #[test]
    fn test_validate_content_rejects_unknown_attribute() {
        let (doc_type, config, title_attr) = make_test_setup();
        let extra_attr = test_attr_id("non-existent");

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Valid Title".into()))),
        );
        fields.insert(
            extra_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(100))),
        );

        let errors = validate_content(&doc_type, &draft_content(fields), &config).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DomainError::UnknownAttribute(attr) if *attr == extra_attr
        ));
    }
}
