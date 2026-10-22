//! Connection bridge with the PHP application.
//!
//! A user logged in one interface opens the other without logging in
//! again. The issuing side signs a short-lived token (HS256, key shared
//! through `config/nw-bridge.key`) and sets a random nonce in the
//! `nw_bridge` cookie; the token carries the SHA-256 of that nonce. The
//! receiving side accepts the token only with the matching cookie, so a
//! token sent to someone else cannot log them in (login CSRF).
//! See `docs/adr/0003-pont-de-connexion.md` and `src/NwBridge.php`.

use anyhow::Context;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use nw_domain::entity::EntitySelection;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::session::Session;

pub const NONCE_COOKIE: &str = "nw_bridge";
/// Tokens consumed by this API.
pub const AUDIENCE_API: &str = "nw-itsm";
/// Tokens consumed by the PHP application.
pub const AUDIENCE_LEGACY: &str = "legacy";
const TTL_SECONDS: i64 = 60;
/// Tolerated clock difference between the PHP and Rust servers.
const CLOCK_SKEW_SECONDS: i64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeClaims {
    pub aud: String,
    /// User id.
    pub uid: i32,
    /// Active profile id.
    pub pid: i32,
    /// Selected entity, `None` for every entity of the profile.
    pub ent: Option<i32>,
    pub rec: bool,
    /// SHA-256 (hex) of the nonce cookie.
    pub nonce: String,
    pub iat: i64,
    pub exp: i64,
}

impl BridgeClaims {
    pub fn selection(&self) -> EntitySelection {
        match self.ent {
            None => EntitySelection::All,
            Some(id) => EntitySelection::Entity {
                id,
                recursive: self.rec,
            },
        }
    }
}

pub struct BridgeKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl BridgeKeys {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }

    /// Token and nonce opening the PHP application for the session.
    pub fn issue_for_legacy(&self, session: &Session) -> anyhow::Result<(String, String)> {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let nonce = hex::encode(bytes);
        let now = Utc::now().timestamp();
        let (ent, rec) = match session.selection {
            EntitySelection::All => (None, false),
            EntitySelection::Entity { id, recursive } => (Some(id), recursive),
        };
        let claims = BridgeClaims {
            aud: AUDIENCE_LEGACY.to_owned(),
            uid: session.user.id,
            pid: session.profile.id,
            ent,
            rec,
            nonce: nonce_hash(&nonce),
            iat: now,
            exp: now + TTL_SECONDS,
        };
        let token = jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .context("cannot sign bridge token")?;
        Ok((token, nonce))
    }

    /// Checks a token issued by the PHP application for this API, with
    /// the nonce cookie set by the PHP application.
    pub fn verify_from_legacy(&self, token: &str, nonce: Option<&str>) -> Option<BridgeClaims> {
        self.verify_at(token, AUDIENCE_API, nonce, Utc::now().timestamp())
    }

    fn verify_at(
        &self,
        token: &str,
        audience: &str,
        nonce: Option<&str>,
        now: i64,
    ) -> Option<BridgeClaims> {
        // Times are checked below, against `now`.
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&[audience]);
        validation.set_required_spec_claims(&["exp", "aud"]);
        validation.validate_exp = false;
        let claims = jsonwebtoken::decode::<BridgeClaims>(token, &self.decoding, &validation)
            .ok()?
            .claims;
        let nonce = nonce.filter(|nonce| !nonce.is_empty())?;
        let fresh = claims.exp >= now
            && claims.iat <= now + CLOCK_SKEW_SECONDS
            && claims.exp - claims.iat <= TTL_SECONDS;
        let matches: bool = nonce_hash(nonce)
            .as_bytes()
            .ct_eq(claims.nonce.as_bytes())
            .into();
        (fresh && matches).then_some(claims)
    }
}

fn nonce_hash(nonce: &str) -> String {
    hex::encode(Sha256::digest(nonce.as_bytes()))
}

/// Cookie holding the nonce of a token for the PHP application.
pub fn nonce_cookie(nonce: String, secure: bool) -> Cookie<'static> {
    Cookie::build((NONCE_COOKIE, nonce))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(TTL_SECONDS))
        .build()
}

/// Removes the nonce once used.
pub fn nonce_removal_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build((NONCE_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();
    cookie.make_removal();
    cookie
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-bridge-secret-test-bridge-secret";

    fn claims(aud: &str, nonce: &str, iat: i64) -> BridgeClaims {
        BridgeClaims {
            aud: aud.to_owned(),
            uid: 2,
            pid: 4,
            ent: Some(1),
            rec: true,
            nonce: nonce_hash(nonce),
            iat,
            exp: iat + TTL_SECONDS,
        }
    }

    fn sign(claims: &BridgeClaims, secret: &[u8]) -> String {
        jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(secret),
        )
        .unwrap()
    }

    #[test]
    fn accepts_a_fresh_token_with_its_nonce() {
        let keys = BridgeKeys::new(SECRET);
        let token = sign(&claims(AUDIENCE_API, "nonce", 1_000), SECRET);
        let verified = keys
            .verify_at(&token, AUDIENCE_API, Some("nonce"), 1_030)
            .unwrap();
        assert_eq!(
            verified.selection(),
            EntitySelection::Entity {
                id: 1,
                recursive: true
            }
        );
    }

    #[test]
    fn accepts_tokens_signed_by_the_php_application() {
        // NwBridge::sign() of src/NwBridge.php, for user 2, profile 4,
        // entity 1 with its sub-entities and the nonce "n0nce".
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJudy1pdHNtIiwidWlkIjoyLCJwaWQiOjQsImVudCI6MSwicmVjIjp0cnVlLCJub25jZSI6IjBhOTIxMzNiNDUxMzc4Y2ExZjk5MWU1MDdmZDhmMTUwMTdkMTU4ZTFiODU2ZDc3ZTBiNjIyMzRhN2NmYTg1M2EiLCJpYXQiOjEwMDAsImV4cCI6MTA2MH0.YQsh4hpW1eWSLb5Z4N1dRNKJpOBy7BnfYO1R5LVigSY";
        let claims = BridgeKeys::new(SECRET)
            .verify_at(token, AUDIENCE_API, Some("n0nce"), 1_030)
            .unwrap();
        assert_eq!((claims.uid, claims.pid), (2, 4));
        assert_eq!(
            claims.selection(),
            EntitySelection::Entity {
                id: 1,
                recursive: true
            }
        );
    }

    /// Prints a token for the PHP tests (`tests/units/itsmng/NwBridge.php`):
    /// `cargo test -p nw-server --lib -- --ignored --nocapture php_vector`.
    #[test]
    #[ignore]
    fn php_vector() {
        let mut claims = claims(AUDIENCE_LEGACY, "n0nce", 1_000);
        claims.ent = None;
        claims.rec = false;
        println!("{}", sign(&claims, SECRET));
    }

    #[test]
    fn refuses_other_nonces_audiences_keys_and_times() {
        let keys = BridgeKeys::new(SECRET);
        let token = sign(&claims(AUDIENCE_API, "nonce", 1_000), SECRET);
        assert!(keys.verify_at(&token, AUDIENCE_API, None, 1_030).is_none());
        assert!(
            keys.verify_at(&token, AUDIENCE_API, Some("other"), 1_030)
                .is_none()
        );
        assert!(
            keys.verify_at(&token, AUDIENCE_LEGACY, Some("nonce"), 1_030)
                .is_none()
        );
        assert!(
            keys.verify_at(&token, AUDIENCE_API, Some("nonce"), 1_061)
                .is_none()
        );
        assert!(
            keys.verify_at(&token, AUDIENCE_API, Some("nonce"), 990)
                .is_none()
        );
        let foreign = sign(
            &claims(AUDIENCE_API, "nonce", 1_000),
            b"another-secret-another-secret-another",
        );
        assert!(
            keys.verify_at(&foreign, AUDIENCE_API, Some("nonce"), 1_030)
                .is_none()
        );
        let mut long_lived = claims(AUDIENCE_API, "nonce", 1_000);
        long_lived.exp = 100_000;
        assert!(
            keys.verify_at(
                &sign(&long_lived, SECRET),
                AUDIENCE_API,
                Some("nonce"),
                1_030
            )
            .is_none()
        );
    }
}
