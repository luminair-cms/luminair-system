//! Machine-to-machine service token registry and verification.

use std::collections::HashMap;

use domain::auth::role::RoleId;
use domain::auth::user::ServiceId;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Identity and role bindings associated with a static service bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTokenEntry {
    pub service_id: ServiceId,
    pub role_ids: Vec<RoleId>,
}

/// In-memory registry mapping static bearer tokens to machine service identities.
#[derive(Debug, Clone, Default)]
pub struct ServiceTokenRegistry {
    tokens: HashMap<String, ServiceTokenEntry>,
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl ServiceTokenEntry {
    /// Creates a new service token entry.
    #[must_use]
    pub const fn new(service_id: ServiceId, role_ids: Vec<RoleId>) -> Self {
        Self {
            service_id,
            role_ids,
        }
    }
}

impl ServiceTokenRegistry {
    /// Creates an empty service token registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
        }
    }

    /// Registers a bearer token with its associated service identity and roles.
    pub fn register(
        &mut self,
        token: impl Into<String>,
        service_id: ServiceId,
        role_ids: Vec<RoleId>,
    ) {
        self.tokens
            .insert(token.into(), ServiceTokenEntry::new(service_id, role_ids));
    }

    /// Looks up a bearer token in the registry, returning the associated entry if found.
    #[must_use]
    pub fn verify(&self, token: &str) -> Option<&ServiceTokenEntry> {
        self.tokens.get(token)
    }

    /// Returns `true` if the registry has no registered tokens.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Returns the number of registered tokens.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_register_and_verify_service_token() {
        let mut registry = ServiceTokenRegistry::new();
        assert!(registry.is_empty());

        let service_id = ServiceId::try_new("indexer-daemon").expect("valid service id");
        let role_id = RoleId::new(Uuid::now_v7());
        registry.register("secret-svc-token", service_id.clone(), vec![role_id]);

        assert_eq!(registry.len(), 1);
        let entry = registry
            .verify("secret-svc-token")
            .expect("token must be found");
        assert_eq!(entry.service_id, service_id);
        assert_eq!(entry.role_ids, &[role_id]);
    }

    #[test]
    fn test_verify_unknown_token_returns_none() {
        let registry = ServiceTokenRegistry::new();
        assert_eq!(registry.verify("non-existent-token"), None);
    }
}
