//! Zero-clone response encoding, streaming serializers, and DTO projections.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use rust_decimal::Decimal;
use serde::ser::{Serialize, SerializeMap, Serializer};

use domain::auth::access_request::{AccessRequest, AccessRequestId, AccessRequestStatus};
use domain::auth::role::{RoleId, UserRoleAssignment, UserRoleAssignmentId};
use domain::auth::user::{ActorId, UserId};
use domain::content::header::{
    DocumentHeader, DocumentSnapshot, DocumentStatus, SnapshotHeader, SnapshotId, SnapshotReason,
};
use domain::content::instance::{
    AuditTrail, DocumentInstance, DocumentInstanceId, PublicationState,
};
use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
use domain::schema::attributes::{
    AttributeId, FieldConstraint, FieldDefinition, RelationDefinition, RelationType,
};
use domain::schema::documents::{DocumentKind, DocumentType, DocumentTypeId};
use domain::schema::types::{FieldType, IntegerSize, PrimitiveType};
use domain::system::config::{SystemConfig, SystemConfigId};
use domain::system::locale::LocaleId;

// ---------------------------------------------------------------------------
// Streaming Serializer Wrappers (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Streaming serializer wrapper for a fields map.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SerFieldsMap(pub HashMap<AttributeId, ContentValue>);

/// Streaming serializer wrapper for a [`ContentValue`] reference.
#[derive(Debug, Clone, Copy)]
pub struct SerContentValue<'a>(pub &'a ContentValue);

/// Streaming serializer wrapper for a [`DomainValue`] reference.
#[derive(Debug, Clone, Copy)]
pub struct SerDomainValue<'a>(pub &'a DomainValue);

/// Streaming serializer wrapper for a [`PrimitiveValue`] reference.
#[derive(Debug, Clone, Copy)]
pub struct SerPrimitiveValue<'a>(pub &'a PrimitiveValue);

// ---------------------------------------------------------------------------
// Response DTO Types
// ---------------------------------------------------------------------------

/// Full document instance DTO for content management and editing responses.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInstanceDto {
    pub id: DocumentInstanceId,
    pub document_type_id: DocumentTypeId,
    pub fields: SerFieldsMap,
    pub publication_state: PublicationStateDto,
    pub relations: HashMap<AttributeId, Vec<DocumentInstanceId>>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub populated_relations: HashMap<AttributeId, Vec<DocumentInstanceDto>>,
    pub audit: AuditTrailDto,
}

/// Publication lifecycle state DTO.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PublicationStateDto {
    Draft,
    Published {
        revision: u32,
        published_at: DateTime<Utc>,
        published_by: ActorId,
    },
    Modified {
        live_revision: u32,
        published_at: DateTime<Utc>,
        published_by: ActorId,
        draft_version: u32,
    },
    Unpublished {
        last_published_revision: u32,
        unpublished_at: DateTime<Utc>,
        unpublished_by: ActorId,
    },
}

/// Audit trail metadata DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditTrailDto {
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub updated_at: DateTime<Utc>,
    pub updated_by: ActorId,
    pub version: u32,
}

/// Lightweight document header projection DTO for listings and table views.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHeaderDto {
    pub id: DocumentInstanceId,
    pub document_type_id: DocumentTypeId,
    pub display_values: IndexMap<AttributeId, String>,
    pub display_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relational_display_target: Option<(AttributeId, DocumentInstanceId)>,
    pub status: DocumentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub updated_at: DateTime<Utc>,
    pub updated_by: ActorId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_by: Option<ActorId>,
}

/// Historical document revision snapshot DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshotDto {
    pub id: SnapshotId,
    pub document_type_id: DocumentTypeId,
    pub document_id: DocumentInstanceId,
    pub revision: u32,
    pub fields: SerFieldsMap,
    pub relations: HashMap<AttributeId, Vec<DocumentInstanceId>>,
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub reason: SnapshotReason,
}

/// Historical revision snapshot header listing DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeaderDto {
    pub id: SnapshotId,
    pub document_type_id: DocumentTypeId,
    pub document_id: DocumentInstanceId,
    pub revision: u32,
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub reason: SnapshotReason,
}

/// Document type schema definition DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTypeDto {
    pub id: DocumentTypeId,
    pub kind: DocumentKind,
    pub info: DocumentTypeInfoDto,
    pub options: DocumentTypeOptionsDto,
    pub fields: Vec<FieldDefinitionDto>,
    pub relations: Vec<RelationDefinitionDto>,
}

/// Metadata describing a document type.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTypeInfoDto {
    pub title: String,
    pub singular_name: DocumentTypeId,
    pub plural_name: DocumentTypeId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub display_fields: Vec<AttributeId>,
}

/// Lifecycle options for a document type.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTypeOptionsDto {
    pub draft_and_publish: bool,
}

/// Schema field definition DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDefinitionDto {
    pub id: AttributeId,
    #[serde(rename = "type")]
    pub field_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u8>,
    pub unique: bool,
    pub required: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<FieldConstraintDto>,
}

/// Field constraint descriptor DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldConstraintDto {
    Pattern(String),
    MinLength(usize),
    MaxLength(usize),
    MinInteger(i64),
    MaxInteger(i64),
    MinDecimal(Decimal),
    MaxDecimal(Decimal),
}

/// Schema relation definition DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationDefinitionDto {
    pub id: AttributeId,
    pub relation_type: RelationType,
    pub target: DocumentTypeId,
}

/// Access request review DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessRequestDto {
    pub id: AccessRequestId,
    pub user_id: UserId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub status: AccessRequestStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<ActorId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<DateTime<Utc>>,
    pub assigned_roles: Vec<RoleId>,
}

/// Access request status DTO.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AccessRequestStatusDto {
    Pending,
    Approved,
    Rejected {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// User role assignment DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAssignmentDto {
    pub id: UserRoleAssignmentId,
    pub user_id: UserId,
    pub role_id: RoleId,
    pub granted_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<ActorId>,
}

/// Borrowed zero-clone system configuration DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemConfigDto<'a> {
    pub id: SystemConfigId,
    pub default_locale: &'a str,
    pub available_locales: Vec<&'a str>,
}

/// Borrowed zero-clone active locales DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalesDto<'a> {
    pub default_locale: &'a str,
    pub available_locales: Vec<&'a str>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl<'a> LocalesDto<'a> {
    /// Creates a borrowed locales response DTO without string allocations.
    #[must_use]
    pub fn new(default_locale: &'a LocaleId, available_locales: &'a [LocaleId]) -> Self {
        Self {
            default_locale: default_locale.as_ref(),
            available_locales: available_locales.iter().map(AsRef::as_ref).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations - Serializers
// ---------------------------------------------------------------------------

impl Serialize for SerFieldsMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s_map = serializer.serialize_map(Some(self.0.len()))?;
        for (attr_id, val) in &self.0 {
            s_map.serialize_entry(attr_id.as_ref(), &SerContentValue(val))?;
        }
        s_map.end()
    }
}

impl<'a> Serialize for SerContentValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            ContentValue::Null => serializer.serialize_none(),
            ContentValue::Scalar(dv) => SerDomainValue(dv).serialize(serializer),
            ContentValue::LocalizedText(map) => {
                let mut s_map = serializer.serialize_map(Some(map.len()))?;
                for (loc, text) in map {
                    s_map.serialize_entry(loc.as_ref(), text)?;
                }
                s_map.end()
            }
        }
    }
}

impl<'a> Serialize for SerDomainValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            DomainValue::Primitive(p) => SerPrimitiveValue(p).serialize(serializer),
            DomainValue::Email(e) => serializer.serialize_str(e.as_ref()),
            DomainValue::Url(u) => serializer.serialize_str(u.as_ref()),
            DomainValue::Json(map) => {
                let mut s_map = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map {
                    s_map.serialize_entry(k, &SerPrimitiveValue(v))?;
                }
                s_map.end()
            }
        }
    }
}

impl<'a> Serialize for SerPrimitiveValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            PrimitiveValue::Text(s) | PrimitiveValue::Uid(s) => serializer.serialize_str(s),
            PrimitiveValue::Uuid(u) => serializer.collect_str(u),
            PrimitiveValue::Integer(i) => serializer.serialize_i64(*i),
            PrimitiveValue::Decimal(d) => serializer.collect_str(d),
            PrimitiveValue::Date(d) => serializer.collect_str(&d.format("%Y-%m-%d")),
            PrimitiveValue::DateTime(dt) => serializer.collect_str(&dt.to_rfc3339()),
            PrimitiveValue::Boolean(b) => serializer.serialize_bool(*b),
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations - Move-on-Encode DTO Conversions
// ---------------------------------------------------------------------------

impl From<DocumentInstance> for DocumentInstanceDto {
    fn from(instance: DocumentInstance) -> Self {
        let (fields, publication_state) = instance.content.into_parts();
        let mut rel_dto = HashMap::with_capacity(instance.relations.len());
        for (attr_id, list) in instance.relations {
            let targets = list.into_iter().map(|r| r.target_instance_id).collect();
            rel_dto.insert(attr_id, targets);
        }

        let mut pop_dto = HashMap::with_capacity(instance.populated_relations.len());
        for (attr_id, list) in instance.populated_relations {
            let dtos = list.into_iter().map(DocumentInstanceDto::from).collect();
            pop_dto.insert(attr_id, dtos);
        }

        Self {
            id: instance.id,
            document_type_id: instance.document_type_id,
            fields: SerFieldsMap(fields),
            publication_state: PublicationStateDto::from(publication_state),
            relations: rel_dto,
            populated_relations: pop_dto,
            audit: AuditTrailDto::from(instance.audit),
        }
    }
}

impl From<PublicationState> for PublicationStateDto {
    fn from(state: PublicationState) -> Self {
        match state {
            PublicationState::Draft => Self::Draft,
            PublicationState::Published {
                revision,
                published_at,
                published_by,
            } => Self::Published {
                revision,
                published_at,
                published_by,
            },
            PublicationState::Modified {
                live_revision,
                published_at,
                published_by,
                draft_version,
            } => Self::Modified {
                live_revision,
                published_at,
                published_by,
                draft_version,
            },
            PublicationState::Unpublished {
                last_published_revision,
                unpublished_at,
                unpublished_by,
            } => Self::Unpublished {
                last_published_revision,
                unpublished_at,
                unpublished_by,
            },
        }
    }
}

impl From<AuditTrail> for AuditTrailDto {
    fn from(audit: AuditTrail) -> Self {
        Self {
            created_at: audit.created_at,
            created_by: audit.created_by,
            updated_at: audit.updated_at,
            updated_by: audit.updated_by,
            version: audit.version,
        }
    }
}

impl From<DocumentHeader> for DocumentHeaderDto {
    fn from(header: DocumentHeader) -> Self {
        Self {
            id: header.id,
            document_type_id: header.document_type_id,
            display_values: header.display_values,
            display_title: header.display_title,
            relational_display_target: header.relational_display_target,
            status: header.status,
            slug: header.slug,
            revision: header.revision,
            draft_version: header.draft_version,
            created_at: header.created_at,
            created_by: header.created_by,
            updated_at: header.updated_at,
            updated_by: header.updated_by,
            published_at: header.published_at,
            published_by: header.published_by,
        }
    }
}

impl From<DocumentSnapshot> for DocumentSnapshotDto {
    fn from(snapshot: DocumentSnapshot) -> Self {
        let mut rel_dto = HashMap::with_capacity(snapshot.relations.len());
        for (attr_id, list) in snapshot.relations {
            let targets = list.into_iter().map(|r| r.target_instance_id).collect();
            rel_dto.insert(attr_id, targets);
        }

        Self {
            id: snapshot.id,
            document_type_id: snapshot.document_type_id,
            document_id: snapshot.document_id,
            revision: snapshot.revision,
            fields: SerFieldsMap(snapshot.fields),
            relations: rel_dto,
            created_at: snapshot.created_at,
            created_by: snapshot.created_by,
            reason: snapshot.reason,
        }
    }
}

impl From<SnapshotHeader> for SnapshotHeaderDto {
    fn from(header: SnapshotHeader) -> Self {
        Self {
            id: header.id,
            document_type_id: header.document_type_id,
            document_id: header.document_id,
            revision: header.revision,
            created_at: header.created_at,
            created_by: header.created_by,
            reason: header.reason,
        }
    }
}

impl From<DocumentType> for DocumentTypeDto {
    fn from(doc_type: DocumentType) -> Self {
        let fields = doc_type
            .fields
            .into_iter()
            .map(FieldDefinitionDto::from)
            .collect();
        let relations = doc_type
            .relations
            .into_iter()
            .map(RelationDefinitionDto::from)
            .collect();

        Self {
            id: doc_type.id,
            kind: doc_type.kind,
            info: DocumentTypeInfoDto {
                title: doc_type.info.title.into(),
                singular_name: doc_type.info.singular_name,
                plural_name: doc_type.info.plural_name,
                description: doc_type.info.description.map(Into::into),
                display_fields: doc_type.info.display_fields,
            },
            options: DocumentTypeOptionsDto {
                draft_and_publish: doc_type.options.draft_and_publish,
            },
            fields,
            relations,
        }
    }
}

impl From<FieldDefinition> for FieldDefinitionDto {
    fn from(def: FieldDefinition) -> Self {
        let (field_type, precision, scale) = decompose_field_type(def.field_type);
        let constraints = def
            .constraints
            .into_iter()
            .map(FieldConstraintDto::from)
            .collect();

        Self {
            id: def.id,
            field_type,
            precision,
            scale,
            unique: def.unique,
            required: def.required,
            constraints,
        }
    }
}

impl From<FieldConstraint> for FieldConstraintDto {
    fn from(c: FieldConstraint) -> Self {
        match c {
            FieldConstraint::Pattern(p) => Self::Pattern(p.as_str().to_string()),
            FieldConstraint::MinLength(n) => Self::MinLength(n),
            FieldConstraint::MaxLength(n) => Self::MaxLength(n),
            FieldConstraint::MinInteger(i) => Self::MinInteger(i),
            FieldConstraint::MaxInteger(i) => Self::MaxInteger(i),
            FieldConstraint::MinDecimal(d) => Self::MinDecimal(d),
            FieldConstraint::MaxDecimal(d) => Self::MaxDecimal(d),
        }
    }
}

impl From<RelationDefinition> for RelationDefinitionDto {
    fn from(def: RelationDefinition) -> Self {
        Self {
            id: def.id,
            relation_type: def.relation_type,
            target: def.target,
        }
    }
}

impl From<AccessRequest> for AccessRequestDto {
    fn from(req: AccessRequest) -> Self {
        Self {
            id: req.id,
            user_id: req.user_id,
            email: req.email.map(Into::into),
            name: req.name.map(Into::into),
            requested_at: req.requested_at,
            status: AccessRequestStatusDto::from(req.status),
            reviewed_by: req.reviewed_by,
            reviewed_at: req.reviewed_at,
            assigned_roles: req.assigned_roles,
        }
    }
}

impl From<AccessRequestStatus> for AccessRequestStatusDto {
    fn from(status: AccessRequestStatus) -> Self {
        match status {
            AccessRequestStatus::Pending => Self::Pending,
            AccessRequestStatus::Approved => Self::Approved,
            AccessRequestStatus::Rejected { reason } => Self::Rejected { reason },
        }
    }
}

impl From<UserRoleAssignment> for RoleAssignmentDto {
    fn from(a: UserRoleAssignment) -> Self {
        Self {
            id: a.id,
            user_id: a.user_id,
            role_id: a.role_id,
            granted_at: a.granted_at,
            granted_by: a.granted_by,
        }
    }
}

impl<'a> From<&'a SystemConfig> for SystemConfigDto<'a> {
    fn from(cfg: &'a SystemConfig) -> Self {
        Self {
            id: cfg.id,
            default_locale: cfg.default_locale.as_ref(),
            available_locales: cfg.available_locales.iter().map(AsRef::as_ref).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn decompose_field_type(ft: FieldType) -> (&'static str, Option<u8>, Option<u8>) {
    match ft {
        FieldType::Primitive(PrimitiveType::Uid) => ("uid", None, None),
        FieldType::Primitive(PrimitiveType::Uuid) => ("uuid", None, None),
        FieldType::Primitive(PrimitiveType::Text) => ("text", None, None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)) => ("int16", None, None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)) => ("int32", None, None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)) => ("int64", None, None),
        FieldType::Primitive(PrimitiveType::Decimal { precision, scale }) => {
            ("decimal", Some(precision), Some(scale))
        }
        FieldType::Primitive(PrimitiveType::Date) => ("date", None, None),
        FieldType::Primitive(PrimitiveType::DateTime) => ("dateTime", None, None),
        FieldType::Primitive(PrimitiveType::Boolean) => ("boolean", None, None),
        FieldType::LocalizedText => ("localizedText", None, None),
        FieldType::Email => ("email", None, None),
        FieldType::Url => ("url", None, None),
        FieldType::Json => ("json", None, None),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::content::instance::DocumentContent;
    use domain::schema::documents::{DocumentTypeInfo, DocumentTypeOptions};
    use indexmap::IndexSet;
    use uuid::Uuid;

    #[test]
    fn test_document_instance_dto_serialization() {
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let actor = ActorId::System;
        let now = Utc::now();

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Hello World".into(),
            ))),
        );

        let content = DocumentContent::new(
            fields,
            PublicationState::Published {
                revision: 1,
                published_at: now,
                published_by: actor.clone(),
            },
        );

        let instance = DocumentInstance {
            id: doc_id,
            document_type_id: type_id,
            content,
            relations: HashMap::new(),
            populated_relations: HashMap::new(),
            audit: AuditTrail {
                created_at: now,
                created_by: actor.clone(),
                updated_at: now,
                updated_by: actor,
                version: 1,
            },
        };

        let dto = DocumentInstanceDto::from(instance);
        let json_str = serde_json::to_string(&dto).expect("serialize DocumentInstanceDto");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

        assert_eq!(json["id"], doc_id.to_string());
        assert_eq!(json["fields"]["title"], "Hello World");
        assert_eq!(json["publicationState"]["status"], "published");
        assert_eq!(json["publicationState"]["revision"], 1);
        assert_eq!(json["audit"]["version"], 1);
    }

    #[test]
    fn test_document_header_dto_serialization() {
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let actor = ActorId::System;
        let now = Utc::now();

        let mut display_values = IndexMap::new();
        display_values.insert(
            AttributeId::try_new("title").unwrap(),
            "My Article".to_string(),
        );

        let header = DocumentHeader {
            id: doc_id,
            document_type_id: type_id,
            display_values,
            display_title: "My Article".to_string(),
            relational_display_target: None,
            status: DocumentStatus::Draft,
            slug: Some("my-article".to_string()),
            revision: None,
            draft_version: Some(1),
            created_at: now,
            created_by: actor.clone(),
            updated_at: now,
            updated_by: actor,
            published_at: None,
            published_by: None,
        };

        let dto = DocumentHeaderDto::from(header);
        let json_str = serde_json::to_string(&dto).expect("serialize DocumentHeaderDto");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

        assert_eq!(json["id"], doc_id.to_string());
        assert_eq!(json["displayTitle"], "My Article");
        assert_eq!(json["status"], "draft");
        assert_eq!(json["draftVersion"], 1);
    }

    #[test]
    fn test_document_snapshot_dto_serialization() {
        let snapshot_id = SnapshotId::new(Uuid::now_v7());
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let actor = ActorId::System;
        let now = Utc::now();

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("views").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(42))),
        );

        let snapshot = DocumentSnapshot {
            id: snapshot_id,
            document_type_id: type_id,
            document_id: doc_id,
            revision: 3,
            fields,
            relations: HashMap::new(),
            created_at: now,
            created_by: actor,
            reason: SnapshotReason::Publish,
        };

        let dto = DocumentSnapshotDto::from(snapshot);
        let json_str = serde_json::to_string(&dto).expect("serialize DocumentSnapshotDto");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

        assert_eq!(json["id"], snapshot_id.to_string());
        assert_eq!(json["documentId"], doc_id.to_string());
        assert_eq!(json["revision"], 3);
        assert_eq!(json["fields"]["views"], 42);
        assert_eq!(json["reason"], "publish");
    }

    #[test]
    fn test_system_config_dto_serialization() {
        let config_id = SystemConfigId::new(Uuid::now_v7());
        let locales = vec![
            LocaleId::try_new("en").unwrap(),
            LocaleId::try_new("ru").unwrap(),
        ];
        let default_loc = LocaleId::try_new("en").unwrap();
        let config = SystemConfig::new(config_id, locales, default_loc).expect("valid config");

        let dto = SystemConfigDto::from(&config);
        let json_str = serde_json::to_string(&dto).expect("serialize SystemConfigDto");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

        assert_eq!(json["id"], config_id.to_string());
        assert_eq!(json["defaultLocale"], "en");
        assert_eq!(json["availableLocales"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_document_type_dto_serialization() {
        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("sku").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Uid),
            unique: true,
            required: true,
            constraints: std::collections::HashSet::new(),
        });

        let mut relations = IndexSet::new();
        relations.insert(RelationDefinition {
            id: AttributeId::try_new("category").unwrap(),
            relation_type: RelationType::BelongsToOne,
            target: DocumentTypeId::try_new("category").unwrap(),
        });

        let doc_type = DocumentType {
            id: DocumentTypeId::try_new("product").unwrap(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Product").unwrap(),
                singular_name: DocumentTypeId::try_new("product").unwrap(),
                plural_name: DocumentTypeId::try_new("products").unwrap(),
                description: None,
                display_fields: vec![AttributeId::try_new("sku").unwrap()],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
            relations,
        };

        let dto = DocumentTypeDto::from(doc_type);
        let json_str = serde_json::to_string(&dto).expect("serialize DocumentTypeDto");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

        assert_eq!(json["id"], "product");
        assert_eq!(json["kind"], "collection");
        assert_eq!(json["fields"][0]["id"], "sku");
        assert_eq!(json["fields"][0]["type"], "uid");
        assert_eq!(json["relations"][0]["id"], "category");
        assert_eq!(json["relations"][0]["relationType"], "belongsToOne");
    }

    #[test]
    fn test_access_request_dto_serialization() {
        let req_id = AccessRequestId::new(Uuid::now_v7());
        let user_id = UserId::try_new("user-99").unwrap();
        let name = DisplayName::try_new("Bob Smith").unwrap();
        let now = Utc::now();

        let req = AccessRequest {
            id: req_id,
            user_id: user_id.clone(),
            email: None,
            name: Some(name),
            requested_at: now,
            status: AccessRequestStatus::Pending,
            reviewed_by: None,
            reviewed_at: None,
            assigned_roles: Vec::new(),
        };

        let dto = AccessRequestDto::from(req);
        let json_str = serde_json::to_string(&dto).expect("serialize AccessRequestDto");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

        assert_eq!(json["id"], req_id.to_string());
        assert_eq!(json["userId"], "user-99");
        assert_eq!(json["name"], "Bob Smith");
        assert_eq!(json["status"]["status"], "pending");
    }
}
