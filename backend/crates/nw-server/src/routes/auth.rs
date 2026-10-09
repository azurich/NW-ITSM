//! Login, logout and current session.

use std::collections::BTreeMap;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use nw_db::{MySqlPool, config, users};
use nw_domain::{
    naming::user_display_name,
    password::{self, Expiration},
    rights::Interface,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult, ErrorBody},
    session::{ProfileChoice, Session, removal_cookie},
    state::AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/session", get(current_session))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionUser {
    pub id: i32,
    pub login: String,
    pub display_name: String,
    /// Preferred language (`fr_FR`...), if set.
    pub language: Option<String>,
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
    /// Every entity is visible.
    pub unrestricted: bool,
    /// Number of visible entities.
    pub count: usize,
}

impl From<&Session> for SessionInfo {
    fn from(session: &Session) -> Self {
        let user = &session.user;
        Self {
            user: SessionUser {
                id: user.id,
                login: user.name.clone().unwrap_or_default(),
                display_name: user_display_name(
                    user.name.as_deref(),
                    user.realname.as_deref(),
                    user.firstname.as_deref(),
                ),
                language: user
                    .language
                    .clone()
                    .filter(|language| !language.is_empty()),
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
                unrestricted: session.scope.is_unrestricted(),
                count: session.scope.active().len(),
            },
            rights: session
                .rights
                .iter()
                .map(|(module, rights)| (module.to_owned(), rights))
                .collect(),
        }
    }
}

/// Logs in with a local ITSM-NG account and sets the session cookie.
#[utoipa::path(
    post,
    path = "/api/v2/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in, `nw_session` cookie set", body = SessionInfo),
        (status = 401, description = "Wrong login or password", body = ErrorBody),
        (status = 403, description = "Account disabled, password expired or no usable profile", body = ErrorBody),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let login = request.login.trim().to_owned();
    let password = request.password;

    let Some(user) = users::find_local_by_login(&state.pool, &login).await? else {
        blocking(move || password::equalize_timing(&password)).await?;
        return Err(ApiError::InvalidCredentials);
    };
    let hash = user.password.clone().unwrap_or_default();
    if !blocking(move || password::verify(&password, &hash)).await? {
        tracing::info!(%login, "login failed");
        return Err(ApiError::InvalidCredentials);
    }

    let now = Utc::now();
    if !user.is_allowed_at(now) {
        return Err(ApiError::AccountDisabled);
    }
    match password_expiration(&state.pool, &user).await? {
        Expiration::Valid => {}
        Expiration::Expired => return Err(ApiError::PasswordExpired),
        Expiration::Locked => return Err(ApiError::AccountDisabled),
    }

    let session = Session::open(&state.pool, user, ProfileChoice::Default).await?;
    let token = state.keys.issue(session.user.id, session.profile.id)?;
    tracing::info!(%login, user = session.user.id, profile = session.profile.id, "login");

    let jar = jar.add(state.keys.cookie(token, state.cookie_secure));
    Ok((jar, Json(SessionInfo::from(&session))))
}

/// Removes the session cookie.
#[utoipa::path(
    post,
    path = "/api/v2/auth/logout",
    tag = "auth",
    responses((status = 204, description = "Session cookie removed"))
)]
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> (CookieJar, StatusCode) {
    (
        jar.add(removal_cookie(state.cookie_secure)),
        StatusCode::NO_CONTENT,
    )
}

/// Returns the current session.
#[utoipa::path(
    get,
    path = "/api/v2/auth/session",
    tag = "auth",
    responses(
        (status = 200, description = "Current session", body = SessionInfo),
        (status = 401, description = "Not logged in", body = ErrorBody),
    )
)]
pub async fn current_session(session: Session) -> Json<SessionInfo> {
    Json(SessionInfo::from(&session))
}

async fn password_expiration(pool: &MySqlPool, user: &users::User) -> ApiResult<Expiration> {
    const DELAY: &str = "password_expiration_delay";
    const LOCK_DELAY: &str = "password_expiration_lock_delay";
    let values = config::core_values(pool, &[DELAY, LOCK_DELAY]).await?;
    let days = |name| {
        values
            .get(name)
            .and_then(|value| value.parse().ok())
            .unwrap_or(-1)
    };
    Ok(password::expiration(
        user.password_last_update,
        days(DELAY),
        days(LOCK_DELAY),
        Utc::now(),
    ))
}

/// Runs CPU-heavy work (bcrypt) outside of the async executor.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    Ok(tokio::task::spawn_blocking(work)
        .await
        .context("password verification task failed")?)
}
