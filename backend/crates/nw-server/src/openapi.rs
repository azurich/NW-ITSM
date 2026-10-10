//! OpenAPI description of `/api/v2`, the contract shared with the web
//! application (`frontend/src/api/schema.ts` is generated from it).

use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use crate::{
    error::{ErrorBody, ErrorDetail},
    routes::{assets, auth, health, session},
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "NW-ITSM API",
        description = "API v2 of NW-ITSM, served by the Rust backend next to the legacy PHP application.",
        license(name = "GPL-2.0-or-later"),
    ),
    paths(
        health::health,
        auth::login,
        auth::logout,
        auth::change_expired_password,
        auth::bridge_from_legacy,
        auth::bridge_to_legacy,
        session::current_session,
        session::switch_profile,
        session::selectable,
        session::switch_entity,
        session::change_password,
        session::update_preferences,
        session::list_languages,
        assets::list,
        assets::get_one,
        assets::create,
        assets::update,
        assets::trash,
        assets::restore,
        assets::purge,
        assets::templates::list,
        assets::templates::get_one,
        assets::templates::create,
        assets::templates::update,
        assets::templates::purge,
        assets::templates::create_item,
        assets::choices::choices,
    ),
    components(schemas(ErrorBody, ErrorDetail, assets::AssetType, assets::choices::ChoiceField)),
    modifiers(&SessionSecurity),
    security(("session_cookie" = []), ("bearer" = [])),
    tags(
        (name = "system", description = "Service status"),
        (name = "auth", description = "Login, logout and the bridge with the classic interface"),
        (name = "session", description = "Current session: profile, entities, password, preferences"),
        (name = "assets", description = "Inventory of assets"),
    )
)]
pub struct ApiDoc;

struct SessionSecurity;

impl Modify for SessionSecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(
                crate::session::COOKIE_NAME,
            ))),
        );
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

/// The OpenAPI document as pretty JSON, with a trailing newline.
pub fn json() -> String {
    let mut json = ApiDoc::openapi()
        .to_pretty_json()
        .expect("the OpenAPI document is serializable");
    json.push('\n');
    json
}
