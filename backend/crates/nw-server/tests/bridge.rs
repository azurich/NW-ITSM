//! API tests on a real database: the connection bridge with the PHP
//! application (`front/nw_bridge.php`).

mod common;

use axum::http::{Method, StatusCode, header};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use nw_server::bridge::BridgeClaims;
use serde_json::json;
use sha2::{Digest, Sha256};

fn nonce_hash(nonce: &str) -> String {
    hex::encode(Sha256::digest(nonce.as_bytes()))
}

/// A token as the PHP application signs it.
fn legacy_token(uid: i32, pid: i32, ent: Option<i32>, nonce: &str, secret: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = BridgeClaims {
        aud: "nw-itsm".to_owned(),
        uid,
        pid,
        ent,
        rec: true,
        nonce: nonce_hash(nonce),
        iat: now,
        exp: now + 60,
    };
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

async fn open_bridge(
    app: &common::TestApp,
    token: &str,
    cookie: Option<&str>,
    redirect: &str,
) -> common::Response {
    let uri = format!("/api/v2/auth/bridge?token={token}&redirect={redirect}");
    app.send(Method::GET, &uri, cookie, None).await
}

fn location(response: &common::Response) -> &str {
    response.headers[header::LOCATION].to_str().unwrap()
}

#[tokio::test]
async fn token_from_the_classic_interface_opens_a_session() {
    let app = test_app!();
    // An LDAP account: it cannot log in here with a password, but the
    // classic interface authenticated it.
    let token = legacy_token(17, 1, Some(1), "n0nce", common::BRIDGE_SECRET);
    let response = open_bridge(
        &app,
        &token,
        Some("nw_bridge=n0nce"),
        "/app/assets/computers",
    )
    .await;

    assert_eq!(response.status, StatusCode::SEE_OTHER);
    assert_eq!(location(&response), "/app/assets/computers");
    assert!(
        response
            .set_cookie("nw_bridge")
            .unwrap()
            .contains("Max-Age=0")
    );
    let cookie = response.cookie("nw_session").unwrap();

    let session = app.get("/api/v2/auth/session", Some(&cookie)).await.body;
    assert_eq!(session["user"]["login"], "ldapuser");
    assert_eq!(
        session["entities"]["selected"],
        json!({ "id": 1, "name": "Root > Paris" })
    );
    assert_eq!(session["entities"]["recursive"], true);
    // Its password belongs to the directory.
    assert_eq!(session["capabilities"]["change_password"], false);
    let change = app
        .send(
            Method::POST,
            "/api/v2/auth/password",
            Some(&cookie),
            Some(json!({ "current_password": "secret", "new_password": "N3w-password" })),
        )
        .await;
    assert_eq!(change.error_code(), "external_account");

    let logged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM glpi_events WHERE message = 'ldapuser log in from IP unknown'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(logged, 1);
}

#[tokio::test]
async fn bridge_refuses_tokens_without_their_nonce_or_from_elsewhere() {
    let app = test_app!();
    let failed = "/app/login?bridge=failed";
    let token = legacy_token(10, 1, None, "n0nce", common::BRIDGE_SECRET);

    // Sent to someone else: no nonce cookie, or another one.
    for cookie in [None, Some("nw_bridge=other")] {
        let response = open_bridge(&app, &token, cookie, "/app/").await;
        assert_eq!(location(&response), failed);
        assert!(response.cookie("nw_session").is_none());
    }
    // Signed with another key.
    let forged = legacy_token(
        10,
        1,
        None,
        "n0nce",
        "another-secret-another-secret-another",
    );
    assert_eq!(
        location(&open_bridge(&app, &forged, Some("nw_bridge=n0nce"), "/app/").await),
        failed
    );
    // Disabled account.
    let disabled = legacy_token(14, 1, None, "n0nce", common::BRIDGE_SECRET);
    assert_eq!(
        location(&open_bridge(&app, &disabled, Some("nw_bridge=n0nce"), "/app/").await),
        failed
    );
    // Profile not granted.
    let other_profile = legacy_token(11, 2, None, "n0nce", common::BRIDGE_SECRET);
    assert_eq!(
        location(&open_bridge(&app, &other_profile, Some("nw_bridge=n0nce"), "/app/").await),
        failed
    );
    // A token for the PHP application.
    let session = app.login("admin").await;
    let issued = app
        .send(
            Method::POST,
            "/api/v2/auth/bridge/legacy",
            Some(&session),
            None,
        )
        .await;
    let for_legacy = issued.body["token"].as_str().unwrap();
    let nonce = issued.cookie("nw_bridge").unwrap();
    assert_eq!(
        location(&open_bridge(&app, for_legacy, Some(&nonce), "/app/").await),
        failed
    );

    // Valid, but the redirection leaves the application.
    let response = open_bridge(
        &app,
        &token,
        Some("nw_bridge=n0nce"),
        "https://evil.example/",
    )
    .await;
    assert_eq!(location(&response), "/app/");
}

#[tokio::test]
async fn token_for_the_classic_interface_is_bound_to_a_nonce() {
    let app = test_app!();
    let unauthenticated = app
        .send(Method::POST, "/api/v2/auth/bridge/legacy", None, None)
        .await;
    assert_eq!(unauthenticated.status, StatusCode::UNAUTHORIZED);

    let session = app.login("paris").await;
    let response = app
        .send(
            Method::POST,
            "/api/v2/auth/bridge/legacy",
            Some(&session),
            None,
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    let nonce_cookie = response.set_cookie("nw_bridge").unwrap();
    assert!(nonce_cookie.contains("HttpOnly") && nonce_cookie.contains("Max-Age=60"));
    let nonce = response.cookie("nw_bridge").unwrap();
    let nonce = nonce.trim_start_matches("nw_bridge=");

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["legacy"]);
    let claims = jsonwebtoken::decode::<BridgeClaims>(
        response.body["token"].as_str().unwrap(),
        &DecodingKey::from_secret(common::BRIDGE_SECRET.as_bytes()),
        &validation,
    )
    .unwrap()
    .claims;
    assert_eq!(
        (claims.uid, claims.pid, claims.ent, claims.rec),
        (11, 1, Some(1), true)
    );
    assert_eq!(claims.nonce, nonce_hash(nonce));
    assert_eq!(claims.exp - claims.iat, 60);
}

#[tokio::test]
async fn expired_local_password_cannot_open_the_classic_interface() {
    let app = test_app!();
    let session = app.login("admin").await;
    let ldap = open_bridge(
        &app,
        &legacy_token(17, 1, None, "n0nce", common::BRIDGE_SECRET),
        Some("nw_bridge=n0nce"),
        "/app/",
    )
    .await
    .cookie("nw_session")
    .unwrap();
    // The password expires during the session.
    app.sql("INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'password_expiration_delay', '30')")
        .await;

    let refused = app
        .send(
            Method::POST,
            "/api/v2/auth/bridge/legacy",
            Some(&session),
            None,
        )
        .await;
    assert_eq!(refused.error_code(), "password_expired");
    // Directory accounts have no password date: not concerned.
    let allowed = app
        .send(
            Method::POST,
            "/api/v2/auth/bridge/legacy",
            Some(&ldap),
            None,
        )
        .await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.body);
}
