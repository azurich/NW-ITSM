//! API tests on a real database: hardened login, profile and entity
//! selection, password changes, preferences and their history.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

async fn events(app: &common::TestApp) -> Vec<String> {
    sqlx::query_scalar("SELECT message FROM glpi_events ORDER BY id")
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

async fn stored_hash(app: &common::TestApp, login: &str) -> String {
    sqlx::query_scalar("SELECT password FROM glpi_users WHERE name = ?")
        .bind(login)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

fn strict_policy() -> &'static str {
    "INSERT INTO glpi_configs (context, name, value) VALUES \
     ('core', 'use_password_security', '1'), ('core', 'password_min_length', '8'), \
     ('core', 'password_need_number', '1'), ('core', 'password_need_letter', '1'), \
     ('core', 'password_need_caps', '1'), ('core', 'password_need_symbol', '1')"
}

#[tokio::test]
async fn repeated_failures_block_the_login_for_a_while() {
    let app = test_app!();
    for _ in 0..5 {
        let response = app.login_response("admin", "wrong").await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    }
    // Even the right password is refused until the window ends.
    let blocked = app.login_response("Admin", "secret").await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.error_code(), "too_many_attempts");
    let retry_after: u64 = blocked.headers[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(retry_after > 0 && retry_after <= 15 * 60);
    // Other accounts are not affected.
    app.login("paris").await;
}

#[tokio::test]
async fn logins_are_written_to_the_event_log() {
    let app = test_app!();
    app.login_response("nobody", "secret").await;
    app.login_response("admin", "wrong").await;
    app.login("admin").await;
    assert_eq!(
        events(&app).await,
        [
            "Failed login for nobody from IP unknown",
            "Failed login for admin from IP unknown",
            "admin log in from IP unknown",
        ]
    );
    let last_login: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_login FROM glpi_users WHERE name = 'admin'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(last_login.is_some());
}

#[tokio::test]
async fn obsolete_hashes_are_replaced_at_login() {
    let app = test_app!();
    assert_eq!(stored_hash(&app, "lyon").await.len(), 32);
    app.login("lyon").await;
    let upgraded = stored_hash(&app, "lyon").await;
    assert!(upgraded.starts_with("$2y$10$"), "{upgraded}");
    // Still the same password, and not a password change.
    app.login("lyon").await;
    let last_update: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT password_last_update FROM glpi_users WHERE name = 'lyon'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(last_update, None);
}

#[tokio::test]
async fn session_describes_preferences_and_capabilities() {
    let app = test_app!();
    app.sql("INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'language', 'fr_FR'), ('core', 'names_format', '1')")
        .await;
    let cookie = app.login("admin").await;
    let session = app.get("/api/v2/auth/session", Some(&cookie)).await.body;
    assert_eq!(session["user"]["display_name"], "Alice Admin");
    assert_eq!(session["user"]["language"], "fr_FR");
    assert_eq!(session["user"]["language_is_default"], true);
    assert_eq!(
        session["capabilities"],
        json!({ "change_password": true, "bridge": true })
    );
    assert_eq!(
        session["profiles"],
        json!([{ "id": 2, "name": "Self-Service" }, { "id": 1, "name": "Technician" }])
    );
    assert_eq!(
        session["entities"],
        json!({ "selected": { "id": 0, "name": "Root" }, "recursive": true, "unrestricted": true, "count": 4 })
    );
}

#[tokio::test]
async fn switching_profile_changes_rights() {
    let app = test_app!();
    let cookie = app.login("admin").await;

    let response = app
        .send(
            Method::PUT,
            "/api/v2/auth/session/profile",
            Some(&cookie),
            Some(json!({ "profile_id": 2 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["profile"]["interface"], "helpdesk");
    let helpdesk = response.cookie("nw_session").unwrap();
    let computers = app.get("/api/v2/assets/computers", Some(&helpdesk)).await;
    assert_eq!(computers.status, StatusCode::FORBIDDEN);

    // A profile that is not granted.
    let response = app
        .send(
            Method::PUT,
            "/api/v2/auth/session/profile",
            Some(&helpdesk),
            Some(json!({ "profile_id": 3 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn selecting_entities_restricts_the_items() {
    let app = test_app!();
    let paris = app.login("paris").await;

    let selectable = app.get("/api/v2/auth/session/entities", Some(&paris)).await;
    assert_eq!(
        selectable.body["items"],
        json!([
            { "id": 1, "parent_id": 0, "name": "Paris", "completename": "Root > Paris", "level": 2, "recursive_allowed": true },
            { "id": 3, "parent_id": 1, "name": "Office A", "completename": "Root > Paris > Office A", "level": 3, "recursive_allowed": true },
        ])
    );

    let select = |entity_id: Value, recursive: bool| {
        let paris = paris.clone();
        let app = &app;
        async move {
            app.send(
                Method::PUT,
                "/api/v2/auth/session/entity",
                Some(&paris),
                Some(json!({ "entity_id": entity_id, "recursive": recursive })),
            )
            .await
        }
    };

    let office = select(json!(3), false).await;
    assert_eq!(office.status, StatusCode::OK, "{}", office.body);
    assert_eq!(
        office.body["entities"]["selected"],
        json!({ "id": 3, "name": "Root > Paris > Office A" })
    );
    let office_cookie = office.cookie("nw_session").unwrap();
    assert_eq!(
        app.names("/api/v2/assets/computers", &office_cookie).await,
        ["PC-PARIS-A", "PC-ROOT-SHARED"]
    );

    let paris_only = select(json!(1), false).await.cookie("nw_session").unwrap();
    assert_eq!(
        app.names("/api/v2/assets/computers", &paris_only).await,
        ["100%_PC", "PC-PARIS-1", "PC-ROOT-SHARED"]
    );

    let all = select(Value::Null, false).await;
    assert_eq!(all.body["entities"]["selected"], Value::Null);
    assert_eq!(all.body["entities"]["count"], 2);

    // Lyon and the root are not granted to this profile.
    assert_eq!(select(json!(2), false).await.status, StatusCode::FORBIDDEN);
    assert_eq!(select(json!(0), true).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn changing_the_password_ends_other_sessions() {
    let app = test_app!();
    app.sql(strict_policy()).await;
    let cookie = app.login("admin").await;
    let other_session = app.login("admin").await;
    let change = |current: &str, new: &str| {
        let body = json!({ "current_password": current, "new_password": new });
        let cookie = cookie.clone();
        let app = &app;
        async move {
            app.send(
                Method::POST,
                "/api/v2/auth/password",
                Some(&cookie),
                Some(body),
            )
            .await
        }
    };

    let wrong = change("wrong", "N3w-password").await;
    assert_eq!(wrong.error_code(), "wrong_password");
    let same = change("secret", "secret").await;
    assert_eq!(same.error_code(), "same_password");
    let weak = change("secret", "short").await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    assert_eq!(weak.error_code(), "weak_password");
    assert_eq!(
        weak.body["error"]["details"],
        json!([
            "too_short",
            "missing_digit",
            "missing_uppercase",
            "missing_symbol"
        ])
    );

    let changed = change("secret", "N3w-password").await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    let renewed = changed.cookie("nw_session").unwrap();
    assert_eq!(
        app.get("/api/v2/auth/session", Some(&renewed)).await.status,
        StatusCode::OK
    );
    assert_eq!(
        app.get("/api/v2/auth/session", Some(&other_session))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login_response("admin", "N3w-password").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn password_change_needs_the_right() {
    let app = test_app!();
    let cookie = app.login("helpdesk").await;
    let session = app.get("/api/v2/auth/session", Some(&cookie)).await.body;
    assert_eq!(session["capabilities"]["change_password"], false);
    let response = app
        .send(
            Method::POST,
            "/api/v2/auth/password",
            Some(&cookie),
            Some(json!({ "current_password": "secret", "new_password": "N3w-password" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn expired_passwords_can_be_replaced_at_login() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES \
         ('core', 'password_expiration_delay', '30'), \
         ('core', 'password_expiration_lock_delay', '10');
         UPDATE glpi_users SET password_last_update = NOW() WHERE name = 'admin';
         UPDATE glpi_users SET password_last_update = NOW() - INTERVAL 35 DAY WHERE name = 'paris';",
    )
    .await;
    let replace = |login: &str, current: &str| {
        let body =
            json!({ "login": login, "current_password": current, "new_password": "N3w-password" });
        let app = &app;
        async move {
            app.send(
                Method::POST,
                "/api/v2/auth/password/expired",
                None,
                Some(body),
            )
            .await
        }
    };

    assert_eq!(
        replace("paris", "wrong").await.error_code(),
        "invalid_credentials"
    );
    assert_eq!(
        replace("admin", "secret").await.error_code(),
        "password_not_expired"
    );
    assert_eq!(
        replace("lyon", "secret").await.error_code(),
        "account_disabled"
    );

    let replaced = replace("paris", "secret").await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
    assert_eq!(replaced.body["user"]["login"], "paris");
    assert!(replaced.cookie("nw_session").is_some());
    assert_eq!(
        app.login_response("paris", "N3w-password").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn language_preference_is_saved_with_history() {
    let app = test_app!();
    let cookie = app.login("admin").await;
    let update = |language: Value| {
        let cookie = cookie.clone();
        let app = &app;
        async move {
            app.send(
                Method::PATCH,
                "/api/v2/me/preferences",
                Some(&cookie),
                Some(json!({ "language": language })),
            )
            .await
        }
    };

    let french = update(json!("fr_FR")).await;
    assert_eq!(french.status, StatusCode::OK, "{}", french.body);
    assert_eq!(french.body["user"]["language"], "fr_FR");
    assert_eq!(french.body["user"]["language_is_default"], false);
    // Same value: nothing new in the history.
    update(json!("fr_FR")).await;
    let default = update(Value::Null).await;
    assert_eq!(default.body["user"]["language_is_default"], true);
    assert_eq!(update(json!("xx_XX")).await.status, StatusCode::BAD_REQUEST);

    let history: Vec<(String, i32, String, i32, String, String)> = sqlx::query_as(
        "SELECT itemtype, items_id, user_name, id_search_option, old_value, new_value \
         FROM glpi_logs ORDER BY id",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        history,
        [
            (
                "User".into(),
                10,
                "Admin Alice (10)".into(),
                17,
                "".into(),
                "fr_FR".into()
            ),
            (
                "User".into(),
                10,
                "Admin Alice (10)".into(),
                17,
                "fr_FR".into(),
                "".into()
            ),
        ]
    );
    let events = events(&app).await;
    assert_eq!(
        events
            .iter()
            .filter(|e| *e == "admin updates an item")
            .count(),
        2
    );

    let languages = app.get("/api/v2/languages", Some(&cookie)).await.body;
    assert!(
        languages
            .as_array()
            .unwrap()
            .contains(&json!({ "code": "fr_FR", "name": "Français" }))
    );
}
