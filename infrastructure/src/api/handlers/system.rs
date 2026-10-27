//! System configuration and localization route handlers.

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};

use application::services::SystemConfigService;

use crate::api::codec::{LocalesDto, SystemConfigDto};
use crate::api::envelope::JsonData;
use crate::api::state::{HasAuthResolver, HasSystemConfigService};
use crate::auth::Caller;

// ---------------------------------------------------------------------------
// Public Handler Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Returns active system configuration including locales and identifier (`GET /api/v1/system/config`).
pub async fn get_system_config<St>(
    State(state): State<St>,
    Caller(_caller): Caller,
) -> Response
where
    St: HasSystemConfigService + HasAuthResolver,
{
    let config = state.system_config_service().get_config();
    Json(JsonData::new(SystemConfigDto::from(config))).into_response()
}

/// Returns default and available system locales (`GET /api/v1/system/locales`).
pub async fn get_system_locales<St>(
    State(state): State<St>,
    Caller(_caller): Caller,
) -> Response
where
    St: HasSystemConfigService + HasAuthResolver,
{
    let svc = state.system_config_service();
    Json(JsonData::new(LocalesDto::new(
        svc.default_locale(),
        svc.available_locales(),
    )))
    .into_response()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::http::StatusCode;
    use axum::response::Response;
    use domain::auth::user::{ActorId, UserId};
    use domain::schema::SchemaRegistry;
    use domain::system::locale::LocaleId;
    use domain::system::{SystemConfig, SystemConfigId, SystemContext};

    use crate::api::state::HasSystemContext;
    use crate::auth::{AuthContextResolver, AuthResolutionError, AuthenticatedUser};

    #[derive(Clone)]
    struct MockSystemState {
        context: &'static SystemContext,
        resolver: Arc<MockAuthResolver>,
        service: Arc<MockConfigService>,
    }

    impl HasSystemContext for MockSystemState {
        fn system_context(&self) -> &'static SystemContext {
            self.context
        }
    }

    impl HasAuthResolver for MockSystemState {
        type Auth = MockAuthResolver;

        fn auth_resolver(&self) -> &Self::Auth {
            &self.resolver
        }
    }

    impl HasSystemConfigService for MockSystemState {
        type SystemConfig = MockConfigService;

        fn system_config_service(&self) -> &Self::SystemConfig {
            &self.service
        }
    }

    struct MockAuthResolver;

    impl AuthContextResolver for MockAuthResolver {
        async fn resolve_user_identity(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<AuthenticatedUser, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(AuthenticatedUser::new(user_id, None, None))
        }

        async fn resolve_caller(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<application::context::CallerContext, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(application::context::CallerContext::new(
                ActorId::User(user_id),
                vec![],
            ))
        }
    }

    struct MockConfigService {
        config: SystemConfig,
    }

    impl SystemConfigService for MockConfigService {
        fn get_config(&self) -> &SystemConfig {
            &self.config
        }
    }

    fn test_system_context() -> (&'static SystemContext, SystemConfig) {
        let registry = SchemaRegistry::new(vec![]);
        let en = LocaleId::try_new("en").expect("valid locale");
        let uk = LocaleId::try_new("uk").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![en.clone(), uk],
            en,
        )
        .expect("valid config");
        let ctx = Box::leak(Box::new(SystemContext::new(registry, config.clone())));
        (ctx, config)
    }

    #[tokio::test]
    async fn test_get_system_config_handler() {
        let (ctx, config) = test_system_context();
        let state = MockSystemState {
            context: ctx,
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockConfigService { config }),
        };
        let caller = Caller(application::context::CallerContext::new(
            ActorId::User(UserId::try_new("usr_test").unwrap()),
            vec![],
        ));

        let res: Response = get_system_config(State(state), caller).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_system_locales_handler() {
        let (ctx, config) = test_system_context();
        let state = MockSystemState {
            context: ctx,
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockConfigService { config }),
        };
        let caller = Caller(application::context::CallerContext::new(
            ActorId::User(UserId::try_new("usr_test").unwrap()),
            vec![],
        ));

        let res: Response = get_system_locales(State(state), caller).await;
        assert_eq!(res.status(), StatusCode::OK);
    }
}
