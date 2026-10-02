use crate::content::DocumentContent;
use crate::content::values::{ContentValue, DomainValue, PrimitiveValue};
use crate::errors::DomainError;
use crate::schema::{DocumentType, FieldConstraint};
use crate::schema::ids::AttributeId;
use crate::schema::types::FieldType;
use crate::system::config::SystemConfig;
use crate::system::ids::LocaleId;

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
        FieldConstraint::Pattern(pat) => {
            // Compile regex; treat compilation failure as a constraint violation to surface config errors
            match regex::Regex::new(pat) {
                Ok(re) if !re.is_match(s) => {
                    return violated(format!("value '{}' does not match pattern '{}'", s, pat));
                }
                Err(e) => {
                    return violated(format!("invalid regex pattern '{}': {}", pat, e));
                }
                _ => {}
            }
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
