use std::fmt;

use nutype::nutype;
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 255),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Display,
        Serialize,
        Deserialize,
        AsRef,
        Deref,
        Into
    )
)]
pub struct UserId(String);

#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 245),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Display,
        Serialize,
        Deserialize,
        AsRef,
        Deref,
        Into
    )
)]
pub struct ServiceId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ActorId {
    User(UserId),
    Service(ServiceId),
    System,
}

impl ActorId {
    #[must_use]
    pub fn to_urn(&self) -> String {
        self.to_string()
    }

    /// Parses an [`ActorId`] from its URN representation (`user:...`, `service:...`, or `system`).
    ///
    /// # Errors
    /// Returns [`DomainError::Validation`] if the URN format or underlying identifier is invalid.
    pub fn from_urn(urn: &str) -> Result<Self, DomainError> {
        if urn == "system" {
            Ok(ActorId::System)
        } else if let Some(rest) = urn.strip_prefix("user:") {
            UserId::try_new(rest)
                .map(ActorId::User)
                .map_err(|e| DomainError::Validation(format!("invalid user actor urn: {e}")))
        } else if let Some(rest) = urn.strip_prefix("service:") {
            ServiceId::try_new(rest)
                .map(ActorId::Service)
                .map_err(|e| DomainError::Validation(format!("invalid service actor urn: {e}")))
        } else {
            Err(DomainError::Validation(format!(
                "unrecognized actor urn: {urn}"
            )))
        }
    }

    #[must_use]
    pub fn as_user(&self) -> Option<&UserId> {
        match self {
            ActorId::User(u) => Some(u),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_service(&self) -> Option<&ServiceId> {
        match self {
            ActorId::Service(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_system(&self) -> bool {
        matches!(self, ActorId::System)
    }
}

impl From<UserId> for ActorId {
    fn from(user_id: UserId) -> Self {
        ActorId::User(user_id)
    }
}

impl From<ServiceId> for ActorId {
    fn from(service_id: ServiceId) -> Self {
        ActorId::Service(service_id)
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorId::User(u) => write!(f, "user:{u}"),
            ActorId::Service(s) => write!(f, "service:{s}"),
            ActorId::System => write!(f, "system"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id_trims_whitespace() {
        let user_id = UserId::try_new("  sub123  ").expect("valid user id");
        assert_eq!(user_id.as_ref(), "sub123");
    }

    #[test]
    fn test_user_id_empty_rejected() {
        assert!(UserId::try_new("").is_err());
        assert!(UserId::try_new("   ").is_err());
    }

    #[test]
    fn test_service_id_trims_whitespace() {
        let service_id = ServiceId::try_new("  service-svc  ").expect("valid service id");
        assert_eq!(service_id.as_ref(), "service-svc");
    }

    #[test]
    fn test_service_id_empty_rejected() {
        assert!(ServiceId::try_new("").is_err());
        assert!(ServiceId::try_new("   ").is_err());
    }

    #[test]
    fn test_service_id_max_len_enforced() {
        let max_str = "a".repeat(245);
        assert!(ServiceId::try_new(&max_str).is_ok());
        let overflow_str = "a".repeat(246);
        assert!(ServiceId::try_new(&overflow_str).is_err());
    }

    #[test]
    fn test_actor_id_from_conversions() {
        let user_id = UserId::try_new("usr_1").expect("valid user id");
        let service_id = ServiceId::try_new("svc_1").expect("valid service id");

        let actor_user: ActorId = user_id.clone().into();
        assert_eq!(actor_user, ActorId::User(user_id));

        let actor_service: ActorId = service_id.clone().into();
        assert_eq!(actor_service, ActorId::Service(service_id));
    }

    #[test]
    fn test_actor_id_urn_roundtrips() {
        let cases = vec![
            (
                ActorId::User(UserId::try_new("alice-123").expect("valid")),
                "user:alice-123",
            ),
            (
                ActorId::Service(ServiceId::try_new("sync-worker").expect("valid")),
                "service:sync-worker",
            ),
            (ActorId::System, "system"),
        ];

        for (actor, expected_urn) in cases {
            assert_eq!(actor.to_urn(), expected_urn);
            assert_eq!(actor.to_string(), expected_urn);
            let parsed = ActorId::from_urn(expected_urn).expect("must parse urn");
            assert_eq!(parsed, actor);
        }
    }

    #[test]
    fn test_actor_id_from_urn_invalid() {
        assert!(ActorId::from_urn("unknown:123").is_err());
        assert!(ActorId::from_urn("user:").is_err());
        assert!(ActorId::from_urn("service:").is_err());
        assert!(ActorId::from_urn("").is_err());
    }

    #[test]
    fn test_actor_id_helpers() {
        let u = UserId::try_new("alice").expect("valid");
        let s = ServiceId::try_new("cron").expect("valid");

        let actor_u = ActorId::User(u.clone());
        assert_eq!(actor_u.as_user(), Some(&u));
        assert_eq!(actor_u.as_service(), None);
        assert!(!actor_u.is_system());

        let actor_s = ActorId::Service(s.clone());
        assert_eq!(actor_s.as_user(), None);
        assert_eq!(actor_s.as_service(), Some(&s));
        assert!(!actor_s.is_system());

        let actor_sys = ActorId::System;
        assert_eq!(actor_sys.as_user(), None);
        assert_eq!(actor_sys.as_service(), None);
        assert!(actor_sys.is_system());
    }

    #[test]
    fn test_actor_id_serde_tagged() {
        let u = ActorId::User(UserId::try_new("alice").expect("valid"));
        let json = serde_json::to_string(&u).expect("serialize");
        assert_eq!(json, r#"{"kind":"user","id":"alice"}"#);
        let deserialized: ActorId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, u);

        let sys = ActorId::System;
        let json = serde_json::to_string(&sys).expect("serialize");
        assert_eq!(json, r#"{"kind":"system"}"#);
        let deserialized: ActorId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, sys);
    }
}
