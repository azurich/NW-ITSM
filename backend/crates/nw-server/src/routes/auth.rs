//! Login, logout, expired passwords and the bridge with the PHP application.

use std::time::Instant;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use nw_db::{
    MySqlPool, config, events,
    users::{self, User},
};
use nw_domain::{
    i18n::{self, text},
    password::{self, Expiration, PasswordPolicy},
    text::to_stored,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    bridge::{NONCE_COOKIE, nonce_cookie, nonce_removal_cookie},
    client::{AcceptLanguage, ClientAddress},
    error::{ApiError, ApiResult, ErrorBody},
    routes::session::SessionInfo,
    session::{ProfileChoice, Session, removal_cookie},
    state::AppState,
};

/// Longest accepted login (`glpi_users.name` is a VARCHAR(255)).
const MAX_LOGIN_LENGTH: usize = 255;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/password/expired", post(change_expired_password))
        .route("/auth/bridge", get(bridge_from_legacy))
        .route("/auth/bridge/legacy", post(bridge_to_legacy))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

/// Logs in with a local ITSM-NG account and sets the session cookie.
///
/// After 5 failures for a login from an address, or 20 from an address,
/// attempts are refused for 15 minutes.
#[utoipa::path(
    post,
    path = "/api/v2/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in, `nw_session` cookie set", body = SessionInfo),
        (status = 401, description = "Wrong login or password", body = ErrorBody),
        (status = 403, description = "Account disabled, password expired or no usable profile", body = ErrorBody),
        (status = 429, description = "Too many failures, see `Retry-After`", body = ErrorBody),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ClientAddress(address): ClientAddress,
    AcceptLanguage(accept): AcceptLanguage,
    jar: CookieJar,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let login = parse_login(&request.login)?;
    let client = LoginClient::new(&state, address, accept.as_deref()).await;
    let user = authenticate(&state, &login, &client, &request.password).await?;
    check_password_policy(&state.pool, &user).await?;

    let session = Session::open(&state.pool, user, ProfileChoice::Default, None).await?;
    finish_login(&state, &session, &client, &login).await?;
    let cookie = state.keys.cookie(&session, state.cookie_secure)?;
    Ok((
        jar.add(cookie),
        Json(SessionInfo::build(&state, &session).await?),
    ))
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExpiredPasswordChange {
    pub login: String,
    pub current_password: String,
    pub new_password: String,
}

/// Replaces an expired password and logs in (legacy `updatepassword.php`).
#[utoipa::path(
    post,
    path = "/api/v2/auth/password/expired",
    tag = "auth",
    request_body = ExpiredPasswordChange,
    responses(
        (status = 200, description = "Password changed, logged in", body = SessionInfo),
        (status = 400, description = "Password not expired, same or weak new password", body = ErrorBody),
        (status = 401, description = "Wrong login or password", body = ErrorBody),
        (status = 403, description = "Account disabled or locked", body = ErrorBody),
        (status = 429, description = "Too many failures, see `Retry-After`", body = ErrorBody),
    )
)]
pub async fn change_expired_password(
    State(state): State<AppState>,
    ClientAddress(address): ClientAddress,
    AcceptLanguage(accept): AcceptLanguage,
    jar: CookieJar,
    payload: Result<Json<ExpiredPasswordChange>, JsonRejection>,
) -> ApiResult<(CookieJar, Json<SessionInfo>)> {
    let Json(request) = payload.map_err(|rejection| ApiError::BadRequest(rejection.body_text()))?;
    let login = parse_login(&request.login)?;
    let client = LoginClient::new(&state, address, accept.as_deref()).await;
    let user = authenticate(&state, &login, &client, &request.current_password).await?;
    match check_password_policy(&state.pool, &user).await {
        Err(ApiError::PasswordExpired) => {}
        Ok(()) => return Err(ApiError::PasswordNotExpired),
        Err(other) => return Err(other),
    }
    set_new_password(
        &state,
        &user,
        &request.current_password,
        &request.new_password,
    )
    .await?;

    let user = users::find_by_id(&state.pool, user.id)
        .await?
        .ok_or(ApiError::InvalidCredentials)?;
    let session = Session::open(&state.pool, user, ProfileChoice::Default, None).await?;
    finish_login(&state, &session, &client, &login).await?;
    let cookie = state.keys.cookie(&session, state.cookie_secure)?;
    Ok((
        jar.add(cookie),
        Json(SessionInfo::build(&state, &session).await?),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BridgeParams {
    /// Token signed by the PHP application.
    pub token: String,
    /// Page of the application to open (`/app/...`).
    pub redirect: Option<String>,
}

/// Opens a session from the PHP application (`front/nw_bridge.php`) and
/// redirects to the application. On failure, redirects to the login page.
#[utoipa::path(
    get,
    path = "/api/v2/auth/bridge",
    tag = "auth",
    params(BridgeParams),
    responses((status = 303, description = "Redirection to the application or to the login page"))
)]
pub async fn bridge_from_legacy(
    State(state): State<AppState>,
    ClientAddress(address): ClientAddress,
    AcceptLanguage(accept): AcceptLanguage,
    jar: CookieJar,
    Query(params): Query<BridgeParams>,
) -> Response {
    let client = LoginClient::new(&state, address, accept.as_deref()).await;
    let nonce = jar
        .get(NONCE_COOKIE)
        .map(|cookie| cookie.value().to_owned());
    let jar = jar.add(nonce_removal_cookie(state.cookie_secure));
    match open_bridged_session(&state, &client, &params.token, nonce.as_deref()).await {
        Ok(session) => match state.keys.cookie(&session, state.cookie_secure) {
            Ok(cookie) => {
                let target = safe_app_path(params.redirect.as_deref());
                (jar.add(cookie), Redirect::to(target)).into_response()
            }
            Err(error) => ApiError::from(error).into_response(),
        },
        Err(error) => {
            tracing::info!(error = %error, "bridge login refused");
            (jar, Redirect::to("/app/login?bridge=failed")).into_response()
        }
    }
}

async fn open_bridged_session(
    state: &AppState,
    client: &LoginClient,
    token: &str,
    nonce: Option<&str>,
) -> ApiResult<Session> {
    let keys = state.bridge.as_ref().ok_or(ApiError::BridgeDisabled)?;
    let claims = keys
        .verify_from_legacy(token, nonce)
        .ok_or(ApiError::Unauthenticated)?;
    let user = users::find_by_id(&state.pool, claims.uid)
        .await?
        .filter(|user| user.is_allowed_at(Utc::now()))
        .ok_or(ApiError::AccountDisabled)?;
    let session = Session::open(
        &state.pool,
        user,
        ProfileChoice::Exact(claims.pid),
        Some(claims.selection()),
    )
    .await?;
    let login = session.user.name.clone().unwrap_or_default();
    finish_login(state, &session, client, &login).await?;
    Ok(session)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BridgeToken {
    /// To pass to `front/nw_bridge.php?token=...` within one minute, from
    /// the same browser (a nonce cookie is set).
    pub token: String,
}

/// Issues a token opening the classic interface as the current user.
#[utoipa::path(
    post,
    path = "/api/v2/auth/bridge/legacy",
    tag = "auth",
    responses(
        (status = 200, description = "Token for the PHP application, nonce cookie set", body = BridgeToken),
        (status = 403, description = "Password expired since the login", body = ErrorBody),
        (status = 404, description = "Bridge not configured", body = ErrorBody),
    )
)]
pub async fn bridge_to_legacy(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<BridgeToken>)> {
    let keys = state.bridge.as_ref().ok_or(ApiError::BridgeDisabled)?;
    // Opening the classic interface is a login there: same checks as a
    // login here. The expiration policy only applies to local accounts.
    if session.user.is_local() {
        check_password_policy(&state.pool, &session.user).await?;
    }
    let (token, nonce) = keys.issue_for_legacy(&session)?;
    Ok((
        jar.add(nonce_cookie(nonce, state.cookie_secure)),
        Json(BridgeToken { token }),
    ))
}

/// Checks a password with the login limits. Logs failures like the legacy
/// login and replaces obsolete hashes (MD5, SHA1) by bcrypt.
pub async fn verify_password(
    state: &AppState,
    account: &str,
    address: &str,
    user: &User,
    password: &str,
) -> ApiResult<()> {
    let now = Instant::now();
    state
        .password_guard
        .check(account, address, now)
        .map_err(|retry_after| ApiError::TooManyAttempts { retry_after })?;
    let hash = user.password.clone().unwrap_or_default();
    let candidate = password.to_owned();
    let stored = hash.clone();
    if !blocking(move || password::verify(&candidate, &stored)).await? {
        state.password_guard.failure(account, address, now);
        return Err(ApiError::WrongPassword);
    }
    if password::is_obsolete(&hash) {
        let candidate = password.to_owned();
        let upgraded = blocking(move || password::hash(&candidate)).await?;
        users::upgrade_password_hash(&state.pool, user.id, &hash, &upgraded).await?;
    }
    Ok(())
}

/// Finds a local account and checks its password.
async fn authenticate(
    state: &AppState,
    login: &str,
    client: &LoginClient,
    password: &str,
) -> ApiResult<User> {
    let address = client.address.as_str();
    let now = Instant::now();
    state
        .password_guard
        .check(login, address, now)
        .map_err(|retry_after| ApiError::TooManyAttempts { retry_after })?;

    let Some(user) = users::find_local_by_login(&state.pool, login).await? else {
        let candidate = password.to_owned();
        blocking(move || password::equalize_timing(&candidate)).await?;
        state.password_guard.failure(login, address, now);
        log_failed_login(state, login, client).await;
        return Err(ApiError::InvalidCredentials);
    };
    match verify_password(state, login, address, &user, password).await {
        Ok(()) => Ok(user),
        Err(ApiError::WrongPassword) => {
            log_failed_login(state, login, client).await;
            Err(ApiError::InvalidCredentials)
        }
        Err(other) => Err(other),
    }
}

/// Refuses inactive accounts and expired passwords, like `Session::init`
/// and `Auth::connection_db`.
async fn check_password_policy(pool: &MySqlPool, user: &User) -> ApiResult<()> {
    if !user.is_allowed_at(Utc::now()) {
        return Err(ApiError::AccountDisabled);
    }
    const DELAY: &str = "password_expiration_delay";
    const LOCK_DELAY: &str = "password_expiration_lock_delay";
    let values = config::core_values(pool, &[DELAY, LOCK_DELAY]).await?;
    let days = |name| {
        values
            .get(name)
            .and_then(|value| value.parse().ok())
            .unwrap_or(-1)
    };
    match password::expiration(
        user.password_last_update,
        days(DELAY),
        days(LOCK_DELAY),
        Utc::now(),
    ) {
        Expiration::Valid => Ok(()),
        Expiration::Expired => Err(ApiError::PasswordExpired),
        Expiration::Locked => Err(ApiError::AccountDisabled),
    }
}

/// Checks and stores a new password (legacy `updatepassword.php` rules).
pub async fn set_new_password(
    state: &AppState,
    user: &User,
    current: &str,
    new: &str,
) -> ApiResult<()> {
    if new == current {
        return Err(ApiError::SamePassword);
    }
    let violations = password_policy(&state.pool).await?.violations(new);
    if !violations.is_empty() {
        return Err(ApiError::WeakPassword(
            violations
                .into_iter()
                .map(|violation| violation.code())
                .collect(),
        ));
    }
    let new = new.to_owned();
    let hash = blocking(move || password::hash(&new)).await?;
    users::change_password(&state.pool, user.id, &hash).await?;
    Ok(())
}

async fn password_policy(pool: &MySqlPool) -> ApiResult<PasswordPolicy> {
    let values = config::core_values(
        pool,
        &[
            "use_password_security",
            "password_min_length",
            "password_need_number",
            "password_need_letter",
            "password_need_caps",
            "password_need_symbol",
        ],
    )
    .await?;
    let number = |name: &str| {
        values
            .get(name)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0)
    };
    Ok(PasswordPolicy {
        enabled: number("use_password_security") != 0,
        min_length: number("password_min_length"),
        need_digit: number("password_need_number") != 0,
        need_lowercase: number("password_need_letter") != 0,
        need_uppercase: number("password_need_caps") != 0,
        need_symbol: number("password_need_symbol") != 0,
    })
}

/// A client logging in: its address, and the language of its requests
/// before the login, in which the PHP code writes the login events
/// (`Auth::login` logs them before `Session::init`).
pub struct LoginClient {
    pub address: String,
    pub language: String,
}

impl LoginClient {
    pub async fn new(state: &AppState, address: String, accept_language: Option<&str>) -> Self {
        let default = config::core_values(&state.pool, &["language"])
            .await
            .ok()
            .and_then(|values| values.get("language").cloned())
            .filter(|language| !language.is_empty())
            .unwrap_or_else(|| "en_GB".to_owned());
        Self {
            address,
            language: i18n::preferred_language(accept_language, &default),
        }
    }
}

/// Records a successful login like the legacy `Auth::login`; `typed` is
/// the login as the user typed it, which the PHP code logs.
async fn finish_login(
    state: &AppState,
    session: &Session,
    client: &LoginClient,
    typed: &str,
) -> ApiResult<()> {
    let login = session.user.name.clone().unwrap_or_default();
    state.password_guard.success(&login, &client.address);
    users::record_login(&state.pool, session.user.id).await?;
    let format = i18n::tr(&client.language, text::LOGIN);
    log_event(
        state,
        &events::Event {
            items_id: 0,
            kind: "system",
            service: "login",
            level: 3,
            message: &i18n::sprintf(&format, &[&to_stored(typed), &client.address]),
        },
    )
    .await;
    tracing::info!(%login, user = session.user.id, profile = session.profile.id, "login");
    Ok(())
}

async fn log_failed_login(state: &AppState, login: &str, client: &LoginClient) {
    tracing::info!(%login, address = %client.address, "login failed");
    let format = i18n::tr(&client.language, text::FAILED_LOGIN);
    log_event(
        state,
        &events::Event {
            items_id: 0,
            kind: "system",
            service: "login",
            level: 3,
            message: &i18n::sprintf(&format, &[&to_stored(login), &client.address]),
        },
    )
    .await;
}

/// Writes to the event log; a failure there must not break the request.
pub async fn log_event(state: &AppState, event: &events::Event<'_>) {
    if let Err(error) = events::log(&state.pool, event).await {
        tracing::warn!(%error, "cannot write to the event log");
    }
}

fn parse_login(login: &str) -> ApiResult<String> {
    let login = login.trim();
    if login.is_empty() || login.chars().count() > MAX_LOGIN_LENGTH {
        return Err(ApiError::InvalidCredentials);
    }
    Ok(login.to_owned())
}

/// Only pages of the application: never an open redirect.
fn safe_app_path(redirect: Option<&str>) -> &str {
    match redirect {
        Some(path)
            if (path == "/app" || path.starts_with("/app/"))
                && !path.contains("//")
                && !path.contains('\\') =>
        {
            path
        }
        _ => "/app/",
    }
}

/// Runs CPU-heavy work (bcrypt) outside of the async executor.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    Ok(tokio::task::spawn_blocking(work)
        .await
        .context("password task failed")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_stay_in_the_application() {
        assert_eq!(
            safe_app_path(Some("/app/assets/computers?page=2")),
            "/app/assets/computers?page=2"
        );
        assert_eq!(safe_app_path(Some("/app")), "/app");
        for unsafe_path in [
            "https://evil.example",
            "//evil.example",
            "/app//evil",
            "/front/central.php",
            "/app\\x",
            "/application",
        ] {
            assert_eq!(safe_app_path(Some(unsafe_path)), "/app/", "{unsafe_path}");
        }
        assert_eq!(safe_app_path(None), "/app/");
    }
}
