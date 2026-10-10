//! The current session: profile and entities, password, preferences.

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    routing::{get, patch, post, put},
};
use axum_extra::extract::cookie::CookieJar;
use nw_db::{entities, events, users};
use nw_domain::{
    entity::{EntitySelection, selectable_entities},
    history, languages,
    rights::Interface,
};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;

use crate::{
    client::ClientAddress,
    error::{ApiError, ApiResult, ErrorBody},
    routes::auth::{self, verify_password},
    session::{ProfileChoice, Session},
    state::AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/session", get(current_session))
        .route("/auth/session/profile", put(switch_profile))
        .route("/auth/session/entities", get(selectable))
        .route("/auth/session/entity", put(switch_entity))
        .route("/auth/password", post(change_password))
        .route("/me/preferences", patch(update_preferences))
        .route("/languages", get(list_languages))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionInfo {
    pub user: SessionUser,
    pub profile: SessionProfile,
    /// Every profile granted to the user.
    pub profiles: Vec<ProfileRef>,
    pub entities: SessionEntities,
    /// Rights of the active profile, as legacy bitmasks per module.
    pub rights: BTreeMap<String, i32>,
    pub capabilities: SessionCapabilities,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionUser {
    pub id: i32,
    pub login: String,
    pub display_name: String,
    /// Language in use (`fr_FR`...): the user's choice or the default one.
    pub language: String,
    /// The language is the default one of the application.
    pub language_is_default: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionProfile {
    pub id: i32,
    pub name: String,
    pub interface: SessionInterface,
}

/// Interface opened by the profile.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SessionInterface {
    /// Full interface (technicians, administrators).
    Central,
    /// Self-service interface.
    Helpdesk,
}

impl From<Interface> for SessionInterface {
    fn from(interface: Interface) -> Self {
        match interface {
            Interface::Central => Self::Central,
            Interface::Helpdesk => Self::Helpdesk,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileRef {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionEntities {
    /// Selected entity; `null` when every entity of the profile is selected.
    pub selected: Option<EntityRef>,
    /// The selected entity includes its sub-entities.
    pub recursive: bool,
    /// Every entity is visible.
    pub unrestricted: bool,
    /// Number of visible entities.
    pub count: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntityRef {
    pub id: i32,
    /// Full name ("Root > Child").
    pub name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionCapabilities {
    /// The user can change their password here (local account and right).
    pub change_password: bool,
    /// The classic interface can be opened without logging in again.
    pub bridge: bool,
}

impl SessionInfo {
    pub async fn build(state: &AppState, session: &Session) -> ApiResult<Self> {
        let user = &session.user;
        let (selected, recursive) = match session.selection {
            EntitySelection::All => (None, false),
            EntitySelection::Entity { id, recursive } => (
                Some(EntityRef {
                    id,
                    name: entities::completename(&state.pool, id).await?,
                }),
                recursive,
            ),
        };
        Ok(Self {
            user: SessionUser {
                id: user.id,
                login: user.name.clone().unwrap_or_default(),
                display_name: session.own_name(),
                language: session.preferences.language.clone(),
                language_is_default: !session.preferences.language_is_set,
            },
            profile: SessionProfile {
                id: session.profile.id,
                name: session.profile.name.clone(),
                interface: session.profile.interface.into(),
            },
            profiles: session
                .profiles
                .iter()
                .map(|profile| ProfileRef {
                    id: profile.id,
                    name: profile.name.clone(),
                })
                .collect(),
            entities: SessionEntities {
                selected,
                recursive,
                unrestricted: session.scope.is_unrestricted(),
                count: session.scope.active().len(),
            },
            rights: session
                .rights
                .iter()
                .map(|(module, rights)| (module.to_owned(), rights))
                .collect(),
            capabilities: SessionCapabilities {
                change_password: can_change_password(session),
                bridge: state.bridge.is_some(),
            },
        })
    }
}

/// Like the legacy user form: local accounts with the `password_update` right.
fn can_change_password(session: &Session) -> bool {
    session.user.is_local() && session.rights.has("password_update", 1)
}

/// Returns the current session.
#[utoipa::path(
    get,
    path = "/api/v2/auth/session",
    tag = "session",
    responses(
        (status = 200, description = "Current session", body = SessionInfo),
        (status = 401, description = "Not logged in", body = ErrorBody),
    )
)]
pub async fn current_session(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<SessionInfo>> {
    Ok(Json(SessionInfo::build(&state, &session).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProfileSwitch {
    pub profile_id: i32,
}

/// Activates another profile of the user, with its default entities (like
/// the legacy profile selector).
#[utoipa::path(
    put,
    path = "/api/v2/auth/session/profile",
    tag = "session",
    request_body = ProfileSwitch,
    responses(
        (status = 200, description = "Profile changed, new session cookie set", body = SessionInfo),
        (status = 403, description = "Profile not granted or not usable", body = ErrorBody),
    )
)]
pub async fn switch_profile(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    payload: Result<Json<ProfileSwitch>, JsonRejection>,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    if !session
        .profiles
        .iter()
        .any(|profile| profile.id == request.profile_id)
    {
        return Err(ApiError::Forbidden);
    }
    let switched = Session::open(
        &state.pool,
        session.user,
        ProfileChoice::Exact(request.profile_id),
        None,
    )
    .await
    .map_err(|error| match error {
        ApiError::NoProfile => ApiError::Forbidden,
        other => other,
    })?;
    respond_with_session(&state, jar, &switched).await
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SelectableEntity {
    pub id: i32,
    pub parent_id: Option<i32>,
    pub name: Option<String>,
    /// Full name ("Root > Child").
    pub completename: Option<String>,
    /// Depth in the tree, 1 for the root.
    pub level: i32,
    /// It can be selected with its sub-entities.
    pub recursive_allowed: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SelectableEntities {
    /// Ordered by full name.
    pub items: Vec<SelectableEntity>,
}

/// Lists the entities the active profile can select.
#[utoipa::path(
    get,
    path = "/api/v2/auth/session/entities",
    tag = "session",
    responses(
        (status = 200, description = "Selectable entities", body = SelectableEntities),
        (status = 401, description = "Not logged in", body = ErrorBody),
    )
)]
pub async fn selectable(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<SelectableEntities>> {
    let allowed: BTreeMap<i32, bool> = selectable_entities(&session.tree, &session.granted)
        .into_iter()
        .map(|entity| (entity.id, entity.recursive_allowed))
        .collect();
    let items = entities::list(&state.pool)
        .await?
        .into_iter()
        .filter_map(|entity| {
            let recursive_allowed = *allowed.get(&entity.id)?;
            Some(SelectableEntity {
                id: entity.id,
                parent_id: (entity.entities_id >= 0 && entity.entities_id != entity.id)
                    .then_some(entity.entities_id),
                name: entity.name,
                completename: entity.completename,
                level: entity.level,
                recursive_allowed,
            })
        })
        .collect();
    Ok(Json(SelectableEntities { items }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EntitySwitch {
    /// Entity to select; `null` selects every entity of the profile.
    pub entity_id: Option<i32>,
    /// Include the sub-entities.
    #[serde(default)]
    pub recursive: bool,
}

/// Selects the entities to work on (legacy entity selector).
#[utoipa::path(
    put,
    path = "/api/v2/auth/session/entity",
    tag = "session",
    request_body = EntitySwitch,
    responses(
        (status = 200, description = "Entities changed, new session cookie set", body = SessionInfo),
        (status = 403, description = "Entity not allowed for the profile", body = ErrorBody),
    )
)]
pub async fn switch_entity(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    payload: Result<Json<EntitySwitch>, JsonRejection>,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let selection = match request.entity_id {
        None => EntitySelection::All,
        Some(id) => EntitySelection::Entity {
            id,
            recursive: request.recursive,
        },
    };
    if !selection.is_allowed(&session.tree, &session.granted) {
        return Err(ApiError::Forbidden);
    }
    let profile = ProfileChoice::Exact(session.profile.id);
    let switched = Session::open(&state.pool, session.user, profile, Some(selection)).await?;
    respond_with_session(&state, jar, &switched).await
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordChange {
    pub current_password: String,
    pub new_password: String,
}

/// Changes the password of the current user. The other sessions of the user
/// end; this one gets a new cookie.
#[utoipa::path(
    post,
    path = "/api/v2/auth/password",
    tag = "session",
    request_body = PasswordChange,
    responses(
        (status = 200, description = "Password changed, new session cookie set", body = SessionInfo),
        (status = 400, description = "Wrong current password, same or weak new password", body = ErrorBody),
        (status = 403, description = "External account or no `password_update` right", body = ErrorBody),
        (status = 429, description = "Too many wrong passwords", body = ErrorBody),
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    ClientAddress(address): ClientAddress,
    session: Session,
    jar: CookieJar,
    payload: Result<Json<PasswordChange>, JsonRejection>,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    if !session.user.is_local() {
        return Err(ApiError::ExternalAccount);
    }
    session.require("password_update", 1)?;

    let account = format!("#{}", session.user.id);
    verify_password(
        &state,
        &account,
        &address,
        &session.user,
        &request.current_password,
    )
    .await?;
    auth::set_new_password(
        &state,
        &session.user,
        &request.current_password,
        &request.new_password,
    )
    .await?;

    let user = users::find_by_id(&state.pool, session.user.id)
        .await?
        .ok_or(ApiError::Unauthenticated)?;
    let profile = ProfileChoice::Exact(session.profile.id);
    let renewed = Session::open(&state.pool, user, profile, Some(session.selection)).await?;
    respond_with_session(&state, jar, &renewed).await
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PreferencesUpdate {
    /// Language code (`fr_FR`...), `null` for the default language. Absent:
    /// unchanged.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub language: Option<Option<String>>,
}

/// Distinguishes `"language": null` from an absent field.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

/// Updates the preferences of the current user, with an entry in the
/// user's history.
#[utoipa::path(
    patch,
    path = "/api/v2/me/preferences",
    tag = "session",
    request_body = PreferencesUpdate,
    responses(
        (status = 200, description = "Preferences saved", body = SessionInfo),
        (status = 400, description = "Unknown language", body = ErrorBody),
    )
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    session: Session,
    payload: Result<Json<PreferencesUpdate>, JsonRejection>,
) -> ApiResult<Json<SessionInfo>> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let Some(language) = request.language else {
        return Ok(Json(SessionInfo::build(&state, &session).await?));
    };
    if let Some(code) = &language {
        if !languages::is_known(code) {
            return Err(ApiError::BadRequest(format!("unknown language: {code}")));
        }
    }

    let actor = history::actor(session.user.id, &session.own_name());
    let changed =
        users::change_language(&state.pool, session.user.id, language.as_deref(), &actor).await?;
    if changed {
        let login = session.user.name.clone().unwrap_or_default();
        auth::log_event(
            &state,
            &events::Event {
                items_id: session.user.id,
                kind: "users",
                service: "setup",
                level: 5,
                message: &format!("{login} updates an item"),
            },
        )
        .await;
    }

    let user = users::find_by_id(&state.pool, session.user.id)
        .await?
        .ok_or(ApiError::Unauthenticated)?;
    let profile = ProfileChoice::Exact(session.profile.id);
    let updated = Session::open(&state.pool, user, profile, Some(session.selection)).await?;
    Ok(Json(SessionInfo::build(&state, &updated).await?))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Language {
    /// `fr_FR`...
    pub code: &'static str,
    /// Name in the language itself.
    pub name: &'static str,
}

/// Languages of the application.
#[utoipa::path(
    get,
    path = "/api/v2/languages",
    tag = "session",
    responses((status = 200, description = "Every language", body = [Language]))
)]
pub async fn list_languages(_session: Session) -> Json<Vec<Language>> {
    Json(
        languages::LANGUAGES
            .iter()
            .map(|&(code, name)| Language { code, name })
            .collect(),
    )
}

/// New cookie and description of a session that changed.
async fn respond_with_session(
    state: &AppState,
    jar: CookieJar,
    session: &Session,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let cookie = state.keys.cookie(session, state.cookie_secure)?;
    Ok((
        jar.add(cookie),
        Json(SessionInfo::build(state, session).await?),
    ))
}
