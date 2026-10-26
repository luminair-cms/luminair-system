use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use indexmap::IndexMap;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::common::{Email, Url};
use crate::content::instance::DocumentInstanceId;
use crate::schema::attributes::AttributeId;
use crate::schema::documents::DocumentType;
use crate::schema::types::{FieldType, IntegerSize, PrimitiveType};
use crate::system::LocaleId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentValue {
    Scalar(DomainValue),
    LocalizedText(HashMap<LocaleId, String>),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainValue {
    Primitive(PrimitiveValue),
    Email(Email),
    Url(Url),
    Json(HashMap<String, PrimitiveValue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveValue {
    Text(String),
    Uid(String),
    Uuid(Uuid),
    Integer(i64),
    Decimal(Decimal),
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
    Boolean(bool),
}

impl DomainValue {
    #[must_use]
    pub fn matches_field_type(&self, ft: &FieldType) -> bool {
        match (self, ft) {
            (DomainValue::Primitive(p), FieldType::Primitive(pt)) => p.matches_primitive_type(pt),
            (DomainValue::Email(_), FieldType::Email)
            | (DomainValue::Url(_), FieldType::Url)
            | (DomainValue::Json(_), FieldType::Json) => true,
            _ => false,
        }
    }

    #[must_use]
    pub fn to_display_string(&self) -> Option<String> {
        match self {
            DomainValue::Primitive(p) => Some(p.to_display_string()),
            DomainValue::Email(e) => Some(e.as_ref().to_string()),
            DomainValue::Url(u) => Some(u.as_ref().to_string()),
            DomainValue::Json(_) => None,
        }
    }
}

impl PrimitiveValue {
    #[must_use]
    pub fn matches_primitive_type(&self, pt: &PrimitiveType) -> bool {
        match (self, pt) {
            (PrimitiveValue::Text(_), PrimitiveType::Text)
            | (PrimitiveValue::Uid(_), PrimitiveType::Uid)
            | (PrimitiveValue::Uuid(_), PrimitiveType::Uuid)
            | (PrimitiveValue::Date(_), PrimitiveType::Date)
            | (PrimitiveValue::DateTime(_), PrimitiveType::DateTime)
            | (PrimitiveValue::Boolean(_), PrimitiveType::Boolean) => true,
            (PrimitiveValue::Integer(val), PrimitiveType::Integer(size)) => match size {
                IntegerSize::I16 => (i64::from(i16::MIN)..=i64::from(i16::MAX)).contains(val),
                IntegerSize::I32 => (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(val),
                IntegerSize::I64 => true,
            },
            (PrimitiveValue::Decimal(d), PrimitiveType::Decimal { precision, scale }) => {
                if *precision == 0 || *scale > *precision {
                    return false;
                }
                let actual_scale = u8::try_from(d.scale()).unwrap_or(u8::MAX);
                if actual_scale > *scale {
                    return false;
                }
                // Verify both total digits and integer digits (left of decimal point) fit NUMERIC(precision, scale)
                let mantissa = d.mantissa().abs();
                let num_digits = if mantissa == 0 {
                    1
                } else {
                    u8::try_from(mantissa.ilog10() + 1).unwrap_or(u8::MAX)
                };
                let int_digits = if mantissa == 0 {
                    0
                } else {
                    num_digits.saturating_sub(actual_scale)
                };
                let max_int_digits = precision.saturating_sub(*scale);
                num_digits <= *precision && int_digits <= max_int_digits
            }
            _ => false,
        }
    }

    #[must_use]
    pub fn to_display_string(&self) -> String {
        match self {
            PrimitiveValue::Text(s) | PrimitiveValue::Uid(s) => s.clone(),
            PrimitiveValue::Uuid(u) => u.to_string(),
            PrimitiveValue::Integer(val) => val.to_string(),
            PrimitiveValue::Decimal(d) => d.to_string(),
            PrimitiveValue::Date(d) => d.to_string(),
            PrimitiveValue::DateTime(dt) => dt.to_rfc3339(),
            PrimitiveValue::Boolean(b) => b.to_string(),
        }
    }
}

impl From<DomainValue> for ContentValue {
    fn from(v: DomainValue) -> Self {
        ContentValue::Scalar(v)
    }
}

impl From<PrimitiveValue> for ContentValue {
    fn from(p: PrimitiveValue) -> Self {
        ContentValue::Scalar(DomainValue::Primitive(p))
    }
}

impl From<PrimitiveValue> for DomainValue {
    fn from(p: PrimitiveValue) -> Self {
        DomainValue::Primitive(p)
    }
}

impl From<Email> for DomainValue {
    fn from(e: Email) -> Self {
        DomainValue::Email(e)
    }
}

impl From<Url> for DomainValue {
    fn from(u: Url) -> Self {
        DomainValue::Url(u)
    }
}

/// Extracts scalar and localized scalar values for the document's effective display fields,
/// returning an ordered map of `AttributeId -> String` and a truncated composite `display_title`.
///
/// Singular relations declared in `display_fields` are skipped at write time (they are enriched
/// at read time in CQRS query handlers).
#[must_use]
pub fn extract_display_identity<S: std::hash::BuildHasher>(
    doc_type: &DocumentType,
    instance_id: DocumentInstanceId,
    fields: &HashMap<AttributeId, ContentValue, S>,
    default_locale: &LocaleId,
) -> (IndexMap<AttributeId, String>, String) {
    let mut display_values = IndexMap::new();

    for attr_id in doc_type.effective_display_fields() {
        // Singular relations are skipped at write time and enriched at read time
        if !doc_type.fields.contains(&attr_id) {
            continue;
        }

        match fields.get(&attr_id) {
            Some(ContentValue::Scalar(dv)) => {
                if let Some(s) = dv.to_display_string()
                    && !s.is_empty()
                {
                    display_values.insert(attr_id, s);
                }
            }
            Some(ContentValue::LocalizedText(map)) => {
                let localized_str = map
                    .get(default_locale)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .or_else(|| {
                        let mut entries: Vec<(&LocaleId, &String)> =
                            map.iter().filter(|(_, s)| !s.is_empty()).collect();
                        entries.sort_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()));
                        entries.first().map(|(_, s)| (*s).clone())
                    });
                if let Some(s) = localized_str {
                    display_values.insert(attr_id, s);
                }
            }
            Some(ContentValue::Null) | None => {}
        }
    }

    let display_title = format_display_title(&display_values, instance_id);

    (display_values, display_title)
}

/// Formats an ordered map of display values into a human-readable title joined by `" — "`,
/// falling back to `fallback_id` when empty and truncating safely on character boundaries to 255 characters.
#[must_use]
pub fn format_display_title(
    display_values: &IndexMap<AttributeId, String>,
    fallback_id: DocumentInstanceId,
) -> String {
    if display_values.is_empty() {
        fallback_id.to_string()
    } else {
        let joined = display_values
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join(" — ");
        if joined.len() <= 255 {
            joined
        } else {
            let mut end = 255;
            while !joined.is_char_boundary(end) {
                end -= 1;
            }
            joined[..end].to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        DocumentTypeBuilder, FieldDefinitionBuilder, create_attr_id, create_doc_type_id,
        create_locale_id,
    };

    #[test]
    fn test_matches_field_type_text_ok() {
        let val: DomainValue = PrimitiveValue::Text("hello".to_string()).into();
        assert!(val.matches_field_type(&FieldType::Primitive(PrimitiveType::Text)));
    }

    #[test]
    fn test_matches_field_type_mismatch() {
        let val: DomainValue = PrimitiveValue::Text("hello".to_string()).into();
        assert!(
            !val.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(!val.matches_field_type(&FieldType::Primitive(PrimitiveType::Boolean)));
        assert!(!val.matches_field_type(&FieldType::Email));
    }

    #[test]
    fn test_matches_field_type_integer_all_sizes() {
        let small: DomainValue = PrimitiveValue::Integer(100).into();
        assert!(
            small.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I16
            )))
        );
        assert!(
            small.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(
            small.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            )))
        );

        let overflow_i16: DomainValue = PrimitiveValue::Integer(70000).into();
        assert!(
            !overflow_i16.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I16
            )))
        );
        assert!(
            overflow_i16.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(
            overflow_i16.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            )))
        );

        let overflow_i32: DomainValue = PrimitiveValue::Integer(3_000_000_000).into();
        assert!(
            !overflow_i32.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(
            overflow_i32.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            )))
        );
    }

    #[test]
    fn test_matches_field_type_decimal_precision_and_scale() {
        let ft = FieldType::Primitive(PrimitiveType::Decimal {
            precision: 5,
            scale: 2,
        });

        // Valid: 123.45 has 5 digits and scale 2
        let valid: DomainValue = PrimitiveValue::Decimal(Decimal::new(12345, 2)).into();
        assert!(valid.matches_field_type(&ft));

        // Scale violation: 12.345 has scale 3 > 2
        let scale_overflow: DomainValue = PrimitiveValue::Decimal(Decimal::new(12345, 3)).into();
        assert!(!scale_overflow.matches_field_type(&ft));

        // Precision violation: 1234.56 has 6 digits > 5
        let prec_overflow: DomainValue = PrimitiveValue::Decimal(Decimal::new(123_456, 2)).into();
        assert!(!prec_overflow.matches_field_type(&ft));

        // Integer digits violation: 12345 has 5 integer digits > max allowed (5 - 2 = 3)
        let int_overflow_scale0: DomainValue =
            PrimitiveValue::Decimal(Decimal::new(12345, 0)).into();
        assert!(!int_overflow_scale0.matches_field_type(&ft));

        // Integer digits violation: 1234.5 has 4 integer digits > max allowed (5 - 2 = 3)
        let int_overflow_scale1: DomainValue =
            PrimitiveValue::Decimal(Decimal::new(12345, 1)).into();
        assert!(!int_overflow_scale1.matches_field_type(&ft));

        // Valid small fraction: 0.05 has 0 integer digits <= 3, scale 2 <= 2
        let fraction_valid: DomainValue = PrimitiveValue::Decimal(Decimal::new(5, 2)).into();
        assert!(fraction_valid.matches_field_type(&ft));
    }

    #[test]
    fn test_matches_field_type_json() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), PrimitiveValue::Text("value".to_string()));
        let val = DomainValue::Json(map);
        assert!(val.matches_field_type(&FieldType::Json));
        assert!(!val.matches_field_type(&FieldType::Primitive(PrimitiveType::Text)));
    }

    #[test]
    fn test_matches_field_type_exhaustive() {
        let now = Utc::now();
        let date = now.date_naive();
        let pairs: Vec<(DomainValue, FieldType)> = vec![
            (
                PrimitiveValue::Text("s".into()).into(),
                FieldType::Primitive(PrimitiveType::Text),
            ),
            (
                PrimitiveValue::Uid("slug".into()).into(),
                FieldType::Primitive(PrimitiveType::Uid),
            ),
            (
                PrimitiveValue::Uuid(Uuid::now_v7()).into(),
                FieldType::Primitive(PrimitiveType::Uuid),
            ),
            (
                DomainValue::Email(Email::try_new("test@example.com").unwrap()),
                FieldType::Email,
            ),
            (
                DomainValue::Url(Url::try_new("https://example.com").unwrap()),
                FieldType::Url,
            ),
            (
                PrimitiveValue::Integer(42).into(),
                FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            ),
            (
                PrimitiveValue::Decimal(Decimal::new(100, 2)).into(),
                FieldType::Primitive(PrimitiveType::Decimal {
                    precision: 10,
                    scale: 2,
                }),
            ),
            (
                PrimitiveValue::Date(date).into(),
                FieldType::Primitive(PrimitiveType::Date),
            ),
            (
                PrimitiveValue::DateTime(now).into(),
                FieldType::Primitive(PrimitiveType::DateTime),
            ),
            (
                PrimitiveValue::Boolean(true).into(),
                FieldType::Primitive(PrimitiveType::Boolean),
            ),
            (DomainValue::Json(HashMap::new()), FieldType::Json),
        ];

        for (i, (val, expected_ft)) in pairs.iter().enumerate() {
            assert!(
                val.matches_field_type(expected_ft),
                "Expected pair {i} to match"
            );
            let mismatch_ft = if *expected_ft == FieldType::Primitive(PrimitiveType::Text) {
                FieldType::Primitive(PrimitiveType::Boolean)
            } else {
                FieldType::Primitive(PrimitiveType::Text)
            };
            assert!(!val.matches_field_type(&mismatch_ft));
        }
    }

    #[test]
    fn test_extract_display_identity_single_scalar() {
        let f =
            FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(f)
            .build();
        let instance_id = DocumentInstanceId::new(Uuid::now_v7());
        let default_locale = create_locale_id("en");

        let mut fields = HashMap::new();
        fields.insert(
            create_attr_id("title"),
            ContentValue::Scalar(PrimitiveValue::Text("My Title".into()).into()),
        );

        let (values, title) =
            extract_display_identity(&doc_type, instance_id, &fields, &default_locale);
        assert_eq!(values.len(), 1);
        assert_eq!(
            values.get(&create_attr_id("title")),
            Some(&"My Title".to_string())
        );
        assert_eq!(title, "My Title");
    }

    #[test]
    fn test_extract_display_identity_composite_three_fields() {
        let f1 =
            FieldDefinitionBuilder::new("terminal-id", FieldType::Primitive(PrimitiveType::Text))
                .build();
        let f2 =
            FieldDefinitionBuilder::new("brand-name", FieldType::Primitive(PrimitiveType::Text))
                .build();
        let f3 =
            FieldDefinitionBuilder::new("removing-date", FieldType::Primitive(PrimitiveType::Date))
                .build();

        let doc_type = DocumentTypeBuilder::collection("terminal")
            .with_field(f1)
            .with_field(f2)
            .with_field(f3)
            .with_display_fields(vec![
                create_attr_id("terminal-id"),
                create_attr_id("brand-name"),
                create_attr_id("removing-date"),
            ])
            .build();

        let instance_id = DocumentInstanceId::new(Uuid::now_v7());
        let default_locale = create_locale_id("en");
        let date = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();

        let mut fields = HashMap::new();
        fields.insert(
            create_attr_id("terminal-id"),
            ContentValue::Scalar(PrimitiveValue::Text("T-100".into()).into()),
        );
        fields.insert(
            create_attr_id("brand-name"),
            ContentValue::Scalar(PrimitiveValue::Text("Acme".into()).into()),
        );
        fields.insert(
            create_attr_id("removing-date"),
            ContentValue::Scalar(PrimitiveValue::Date(date).into()),
        );

        let (values, title) =
            extract_display_identity(&doc_type, instance_id, &fields, &default_locale);
        let keys: Vec<_> = values.keys().cloned().collect();
        assert_eq!(
            keys,
            vec![
                create_attr_id("terminal-id"),
                create_attr_id("brand-name"),
                create_attr_id("removing-date"),
            ]
        );
        assert_eq!(title, "T-100 — Acme — 2026-12-31");
    }

    #[test]
    fn test_extract_display_identity_omits_null_and_missing_fields() {
        let f1 =
            FieldDefinitionBuilder::new("terminal-id", FieldType::Primitive(PrimitiveType::Text))
                .build();
        let f2 =
            FieldDefinitionBuilder::new("brand-name", FieldType::Primitive(PrimitiveType::Text))
                .build();
        let f3 =
            FieldDefinitionBuilder::new("removing-date", FieldType::Primitive(PrimitiveType::Date))
                .build();

        let doc_type = DocumentTypeBuilder::collection("terminal")
            .with_field(f1)
            .with_field(f2)
            .with_field(f3)
            .with_display_fields(vec![
                create_attr_id("terminal-id"),
                create_attr_id("brand-name"),
                create_attr_id("removing-date"),
            ])
            .build();

        let instance_id = DocumentInstanceId::new(Uuid::now_v7());
        let default_locale = create_locale_id("en");

        let mut fields = HashMap::new();
        fields.insert(
            create_attr_id("terminal-id"),
            ContentValue::Scalar(PrimitiveValue::Text("T-100".into()).into()),
        );
        fields.insert(create_attr_id("brand-name"), ContentValue::Null);
        // removing-date omitted

        let (values, title) =
            extract_display_identity(&doc_type, instance_id, &fields, &default_locale);
        assert_eq!(values.len(), 1);
        assert_eq!(
            values.get(&create_attr_id("terminal-id")),
            Some(&"T-100".to_string())
        );
        assert_eq!(title, "T-100");
    }

    #[test]
    fn test_extract_display_identity_localized_text_default_and_fallback() {
        let f = FieldDefinitionBuilder::new("headline", FieldType::LocalizedText).build();
        let doc_type = DocumentTypeBuilder::collection("news")
            .with_field(f)
            .build();
        let instance_id = DocumentInstanceId::new(Uuid::now_v7());

        // Case 1: default locale present
        let mut map = HashMap::new();
        map.insert(create_locale_id("en"), "Breaking News".to_string());
        map.insert(create_locale_id("uk"), "Гарячі новини".to_string());

        let mut fields = HashMap::new();
        fields.insert(create_attr_id("headline"), ContentValue::LocalizedText(map));

        let (_, title_en) =
            extract_display_identity(&doc_type, instance_id, &fields, &create_locale_id("en"));
        assert_eq!(title_en, "Breaking News");

        // Case 2: default locale absent -> lexicographically first locale with non-empty string
        let mut map2 = HashMap::new();
        map2.insert(create_locale_id("uk"), "Новини".to_string());
        map2.insert(create_locale_id("de"), "Nachrichten".to_string());

        let mut fields2 = HashMap::new();
        fields2.insert(create_attr_id("headline"), ContentValue::LocalizedText(map2));

        let (_, title_fallback) =
            extract_display_identity(&doc_type, instance_id, &fields2, &create_locale_id("fr"));
        assert_eq!(title_fallback, "Nachrichten");
    }

    #[test]
    fn test_extract_display_identity_skips_singular_relations() {
        use crate::schema::attributes::{RelationDefinition, RelationType};

        let f =
            FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();
        let rel = RelationDefinition {
            id: create_attr_id("author"),
            relation_type: RelationType::HasOne,
            target: create_doc_type_id("author"),
        };

        let doc_type = DocumentTypeBuilder::collection("book")
            .with_field(f)
            .with_relation(rel)
            .with_display_fields(vec![create_attr_id("title"), create_attr_id("author")])
            .build();

        let instance_id = DocumentInstanceId::new(Uuid::now_v7());
        let default_locale = create_locale_id("en");

        let mut fields = HashMap::new();
        fields.insert(
            create_attr_id("title"),
            ContentValue::Scalar(PrimitiveValue::Text("The Great Gatsby".into()).into()),
        );

        let (values, title) =
            extract_display_identity(&doc_type, instance_id, &fields, &default_locale);
        assert_eq!(values.len(), 1);
        assert_eq!(
            values.get(&create_attr_id("title")),
            Some(&"The Great Gatsby".to_string())
        );
        assert!(!values.contains_key(&create_attr_id("author")));
        assert_eq!(title, "The Great Gatsby");
    }

    #[test]
    fn test_extract_display_identity_fallback_to_instance_id() {
        let f =
            FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(f)
            .build();
        let instance_id = DocumentInstanceId::new(Uuid::now_v7());
        let default_locale = create_locale_id("en");

        let fields = HashMap::new(); // no values provided
        let (values, title) =
            extract_display_identity(&doc_type, instance_id, &fields, &default_locale);
        assert!(values.is_empty());
        assert_eq!(title, instance_id.to_string());
    }

    #[test]
    fn test_extract_display_identity_truncation_utf8_safe() {
        let f =
            FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();
        let doc_type = DocumentTypeBuilder::collection("article")
            .with_field(f)
            .build();
        let instance_id = DocumentInstanceId::new(Uuid::now_v7());
        let default_locale = create_locale_id("en");

        // "Ж" is 2 bytes in UTF-8. 150 * 2 = 300 bytes > 255.
        let long_cyrillic = "Ж".repeat(150);
        let mut fields = HashMap::new();
        fields.insert(
            create_attr_id("title"),
            ContentValue::Scalar(PrimitiveValue::Text(long_cyrillic).into()),
        );

        let (values, title) =
            extract_display_identity(&doc_type, instance_id, &fields, &default_locale);
        assert_eq!(values.len(), 1);
        assert!(title.len() <= 255);
        // Ensure valid UTF-8 and even number of bytes since "Ж" is 2 bytes: 254 bytes
        assert_eq!(title.len(), 254);
    }
}
