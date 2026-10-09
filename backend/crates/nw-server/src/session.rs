//! Authenticated sessions.
//!
//! A session is a signed token (HS256) holding the user and the active
//! profile. It is sent as an `HttpOnly` cookie to the web application, or
//! as a `Bearer` token by API clients. The user, profile, rights and
//! entities are reloaded on every request, so disabling an account or
//! removing a profile takes effect immediately.

use std::time::Duration;

use anyhow::Context;
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use nw_db::{
    MySqlPool, entities, profiles,
    users::{self, User},
};
use nw_domain::{
    entity::{EntityScope, ProfileEntity},
    rights::{Interface, Rights},
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

pub const COOKIE_NAME: &str = "nw_session";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// User id.
    uid: i32,
    /// Active profile id.
    pid: i32,
    iat: i64,
    exp: i64,
}

/// Keys signing and verifying session tokens.
pub struct SessionKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    ttl: Duration,
}

impl SessionKeys {
    pub fn new(secret: &[u8], ttl: Duration) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["exp"]);
        validation.leeway = 0;
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation,
            ttl,
        }
    }

    /// Signs a token for a user and its active profile.
    pub fn issue(&self, user_id: i32, profile_id: i32) -> anyhow::Result<String> {
        let now = Utc::now().timestamp();
        let ttl = i64::try_from(self.ttl.as_secs()).context("session TTL too large")?;
        let claims = Claims {
            uid: user_id,
            pid: profile_id,
            iat: now,
            exp: now + ttl,
        };
        jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .context("cannot sign session token")
    }

    fn verify(&self, token: &str) -> Option<Claims> {
        jsonwebtoken::decode(token, &self.decoding, &self.validation)
            .ok()
            .map(|data| data.claims)
    }

    /// Cookie carrying a session token.
    pub fn cookie(&self, token: String, secure: bool) -> Cookie<'static> {
        let max_age = i64::try_from(self.ttl.as_secs()).unwrap_or(i64::MAX);
        Cookie::build((COOKIE_NAME, token))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(secure)
            .max_age(time::Duration::seconds(max_age))
            .build()
    }
}

/// Cookie removing the session from the browser.
pub fn removal_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();
    cookie.make_removal();
    cookie
}

/// The active profile of a session.
#[derive(Debug, Clone)]
pub struct ActiveProfile {
    pub id: i32,
    pub name: String,
    pub interface: Interface,
}

/// A profile the user may switch to.
#[derive(Debug, Clone)]
pub struct AvailableProfile {
    pub id: i32,
    pub name: String,
}

/// Everything known about the authenticated user for one request.
#[derive(Debug, Clone)]
pub struct Session {
    pub user: User,
    pub profile: ActiveProfile,
    pub profiles: Vec<AvailableProfile>,
    pub rights: Rights,
    pub scope: EntityScope,
}

/// Which profile to activate.
#[derive(Debug, Clone, Copy)]
pub enum ProfileChoice {
    /// At login: the user's default profile, or the first one by name.
    Default,
    /// After login: the profile stored in the token, if still granted.
    Exact(i32),
}

impl Session {
    /// Opens a session for a user whose credentials were checked, with the
    /// same profile and entity selection as a legacy login.
    ///
    /// Fails with [`ApiError::NoProfile`] when no usable profile remains.
    pub async fn open(pool: &MySqlPool, user: User, choice: ProfileChoice) -> ApiResult<Self> {
        let grants = profiles::grants_for_user(pool, user.id).await?;

        let profile_id = match choice {
            ProfileChoice::Exact(id) => id,
            ProfileChoice::Default => grants
                .iter()
                .find(|grant| grant.profile_id == user.profiles_id)
                .or_else(|| grants.first())
                .map(|grant| grant.profile_id)
                .ok_or(ApiError::NoProfile)?,
        };
        let profile_grants: Vec<_> = grants
            .iter()
            .filter(|grant| grant.profile_id == profile_id)
            .collect();
        let first = profile_grants.first().ok_or(ApiError::NoProfile)?;
        let interface = first
            .interface
            .as_deref()
            .and_then(Interface::parse)
            .ok_or(ApiError::NoProfile)?;
        let profile = ActiveProfile {
            id: profile_id,
            name: first.profile_name.clone().unwrap_or_default(),
            interface,
        };

        let granted: Vec<_> = profile_grants
            .iter()
            .map(|grant| ProfileEntity {
                entity_id: grant.entity_id,
                is_recursive: grant.is_recursive,
            })
            .collect();
        let tree = entities::tree(pool).await?;
        let scope = EntityScope::for_profile(&tree, &granted, user.entities_id)
            .ok_or(ApiError::NoProfile)?;
        let rights = Rights::new(interface, profiles::rights(pool, profile_id).await?);

        let mut profiles: Vec<AvailableProfile> = Vec::new();
        for grant in &grants {
            if !profiles.iter().any(|p| p.id == grant.profile_id) {
                profiles.push(AvailableProfile {
                    id: grant.profile_id,
                    name: grant.profile_name.clone().unwrap_or_default(),
                });
            }
        }

        Ok(Self {
            user,
            profile,
            profiles,
            rights,
            scope,
        })
    }

    /// Fails with [`ApiError::Forbidden`] unless the active profile has
    /// the right on the module.
    pub fn require(&self, module: &str, right: i32) -> ApiResult<()> {
        if self.rights.has(module, right) {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts)
            .or_else(|| {
                CookieJar::from_headers(&parts.headers)
                    .get(COOKIE_NAME)
                    .map(|cookie| cookie.value().to_owned())
            })
            .ok_or(ApiError::Unauthenticated)?;
        let claims = state.keys.verify(&token).ok_or(ApiError::Unauthenticated)?;

        let user = users::find_by_id(&state.pool, claims.uid)
            .await?
            .filter(|user| user.is_allowed_at(Utc::now()))
            .ok_or(ApiError::Unauthenticated)?;

        Self::open(&state.pool, user, ProfileChoice::Exact(claims.pid))
            .await
            .map_err(|error| match error {
                ApiError::NoProfile => ApiError::Unauthenticated,
                other => other,
            })
    }
}

fn bearer_token(parts: &Parts) -> Option<String> {
    let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim().to_owned())
}
