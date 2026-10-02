use nutype::nutype;
use uuid::Uuid;

use super::locale::LocaleId;
use crate::errors::DomainError;

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct SystemConfigId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemConfig {
    pub id: SystemConfigId,
    pub available_locales: Vec<LocaleId>,
    pub default_locale: LocaleId,
}

impl SystemConfig {
    pub fn new(
        id: SystemConfigId,
        available_locales: Vec<LocaleId>,
        default_locale: LocaleId,
    ) -> Result<Self, DomainError> {
        if !available_locales.contains(&default_locale) {
            return Err(DomainError::UnknownLocale(default_locale));
        }
        Ok(Self {
            id,
            available_locales,
            default_locale,
        })
    }

    pub fn contains_locale(&self, locale: &LocaleId) -> bool {
        self.available_locales.contains(locale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use crate::test_support::test_locales;

    fn test_config_id() -> SystemConfigId {
        SystemConfigId::new(Uuid::now_v7())
    }

    #[test]
    fn test_contains_locale_found() {
        let (en, uk, _) = test_locales();
        let config = SystemConfig::new(test_config_id(), vec![en.clone(), uk.clone()], en).unwrap();
        assert!(config.contains_locale(&uk));
    }

    #[test]
    fn test_contains_locale_not_found() {
        let (en, uk, fr) = test_locales();
        let config = SystemConfig::new(test_config_id(), vec![en.clone(), uk], en).unwrap();
        assert!(!config.contains_locale(&fr));
    }

    #[test]
    fn test_contains_locale_default_included() {
        let (en, uk, fr) = test_locales();
        // default_locale 'fr' is not in [en, uk] -> error
        let res = SystemConfig::new(test_config_id(), vec![en.clone(), uk], fr.clone());
        assert!(matches!(res, Err(DomainError::UnknownLocale(loc)) if loc == fr));

        // valid when default_locale is in available_locales
        let valid = SystemConfig::new(test_config_id(), vec![en.clone()], en.clone()).unwrap();
        assert!(valid.contains_locale(&en));
    }
}
