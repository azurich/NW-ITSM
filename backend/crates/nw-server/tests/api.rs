//! API tests on a real database: login basics and computers.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn health_reports_database() {
    let app = test_app!();
    let response = app.get("/api/v2/health", None).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["database"], "ok");
}

#[tokio::test]
async fn login_sets_a_secure_http_only_cookie() {
    let app = test_app!();
    let response = app.login_response("admin", "secret").await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    let cookie = response.set_cookie("nw_session").unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("SameSite=Lax"));
    assert_eq!(response.body["user"]["display_name"], "Admin Alice");
    assert_eq!(response.body["profile"]["name"], "Technician");
    assert_eq!(response.body["profile"]["interface"], "central");
    assert_eq!(response.body["rights"]["computer"], 3);
    assert_eq!(response.body["entities"]["unrestricted"], true);
}

#[tokio::test]
async fn session_is_restored_from_cookie_or_bearer_token() {
    let app = test_app!();
    let cookie = app.login("paris").await;

    let response = app.get("/api/v2/auth/session", Some(&cookie)).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["user"]["login"], "paris");
    assert_eq!(response.body["entities"]["count"], 2);

    let token = cookie.trim_start_matches("nw_session=");
    let response = app
        .request(
            Request::get("/api/v2/auth/session")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
}

#[tokio::test]
async fn bad_credentials_are_rejected() {
    let app = test_app!();
    for (login, password) in [
        ("admin", "wrong"),
        ("nobody", "secret"),
        ("ldapuser", "secret"),
    ] {
        let response = app.login_response(login, password).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{login}");
        assert_eq!(response.body["error"]["code"], "invalid_credentials");
    }
}

#[tokio::test]
async fn legacy_md5_password_is_accepted() {
    let app = test_app!();
    app.login("lyon").await;
}

#[tokio::test]
async fn inactive_accounts_and_unusable_profiles_cannot_log_in() {
    let app = test_app!();
    for (login, code) in [
        ("disabled", "account_disabled"),
        ("expired", "account_disabled"),
        ("noprofile", "no_profile"),
        ("broken", "no_profile"),
    ] {
        let response = app.login_response(login, "secret").await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{login}");
        assert_eq!(response.body["error"]["code"], code, "{login}");
    }
}

#[tokio::test]
async fn password_expiration_policy_is_enforced() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES \
         ('core', 'password_expiration_delay', '30'), \
         ('core', 'password_expiration_lock_delay', '10');
         UPDATE glpi_users SET password_last_update = NOW() WHERE name = 'admin';
         UPDATE glpi_users SET password_last_update = NOW() - INTERVAL 35 DAY WHERE name = 'paris';",
    )
    .await;

    assert_eq!(
        app.login_response("admin", "secret").await.status,
        StatusCode::OK
    );
    let expired = app.login_response("paris", "secret").await;
    assert_eq!(expired.body["error"]["code"], "password_expired");
    // Never updated: expired for longer than the lock delay.
    let locked = app.login_response("lyon", "secret").await;
    assert_eq!(locked.body["error"]["code"], "account_disabled");
}

#[tokio::test]
async fn invalid_or_missing_session_is_unauthenticated() {
    let app = test_app!();
    let response = app.get("/api/v2/computers", None).await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.body["error"]["code"], "unauthenticated");

    let response = app
        .get("/api/v2/computers", Some("nw_session=forged.token.value"))
        .await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabling_an_account_ends_its_sessions() {
    let app = test_app!();
    let cookie = app.login("admin").await;
    app.sql("UPDATE glpi_users SET is_active = 0 WHERE name = 'admin'")
        .await;
    let response = app.get("/api/v2/auth/session", Some(&cookie)).await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_removes_the_cookie() {
    let app = test_app!();
    let response = app
        .request(
            Request::post("/api/v2/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT);
    let cookie = response.set_cookie("nw_session").unwrap();
    assert!(cookie.starts_with("nw_session=;"), "{cookie}");
    assert!(cookie.contains("Max-Age=0"), "{cookie}");
}

#[tokio::test]
async fn computers_follow_the_entity_scope() {
    let app = test_app!();

    // Root, recursive: everything but the trash and templates.
    let admin = app.login("admin").await;
    let names = app.names("/api/v2/computers?per_page=100", &admin).await;
    assert_eq!(
        names,
        [
            "100%_PC",
            "PC-LYON-1",
            "PC-PARIS-1",
            "PC-PARIS-A",
            "PC-ROOT-PRIVATE",
            "PC-ROOT-SHARED"
        ]
    );

    // Paris, recursive: Paris and Office A, plus root items shared with
    // sub-entities.
    let paris = app.login("paris").await;
    let names = app.names("/api/v2/computers?per_page=100", &paris).await;
    assert_eq!(
        names,
        ["100%_PC", "PC-PARIS-1", "PC-PARIS-A", "PC-ROOT-SHARED"]
    );

    // Lyon, not recursive.
    let lyon = app.login("lyon").await;
    let names = app.names("/api/v2/computers?per_page=100", &lyon).await;
    assert_eq!(names, ["PC-LYON-1", "PC-ROOT-SHARED"]);
}

#[tokio::test]
async fn helpdesk_profiles_cannot_read_computers() {
    let app = test_app!();
    let cookie = app.login("helpdesk").await;
    let response = app.get("/api/v2/computers", Some(&cookie)).await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.body["error"]["code"], "forbidden");
}

#[tokio::test]
async fn computer_detail_respects_scope_trash_and_templates() {
    let app = test_app!();
    let paris = app.login("paris").await;

    let response = app.get("/api/v2/computers/102", Some(&paris)).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["serial"], "SN-P1");
    assert_eq!(response.body["inventory_number"], "INV-P1");
    assert_eq!(
        response.body["entity"],
        json!({ "id": 1, "name": "Root > Paris" })
    );
    assert_eq!(response.body["location"]["name"], "Paris HQ > Room 1");
    assert_eq!(response.body["type"]["name"], "Laptop");
    assert_eq!(
        response.body["user"],
        json!({ "id": 10, "name": "Admin Alice" })
    );
    assert_eq!(response.body["model"], Value::Null);

    let trashed = app.get("/api/v2/computers/105", Some(&paris)).await;
    assert_eq!(trashed.status, StatusCode::OK);
    assert_eq!(trashed.body["is_deleted"], true);

    for hidden in [104, 106, 100, 9999] {
        let response = app
            .get(&format!("/api/v2/computers/{hidden}"), Some(&paris))
            .await;
        assert_eq!(response.status, StatusCode::NOT_FOUND, "computer {hidden}");
    }
    let response = app.get("/api/v2/computers/abc", Some(&paris)).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn computer_search_escapes_wildcards() {
    let app = test_app!();
    let admin = app.login("admin").await;
    assert_eq!(
        app.names("/api/v2/computers?search=100%25_", &admin).await,
        ["100%_PC"]
    );
    assert_eq!(
        app.names("/api/v2/computers?search=SN-P1", &admin).await,
        ["PC-PARIS-1"]
    );
    assert_eq!(
        app.names("/api/v2/computers?search=_", &admin).await,
        ["100%_PC"]
    );
    assert!(
        app.names("/api/v2/computers?search=nothing", &admin)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn computer_list_is_paginated_and_sorted() {
    let app = test_app!();
    let admin = app.login("admin").await;

    let response = app
        .get(
            "/api/v2/computers?per_page=2&page=2&sort=updated_at&order=desc",
            Some(&admin),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["total"], 6);
    assert_eq!(response.body["page"], 2);
    assert_eq!(response.body["per_page"], 2);
    let names: Vec<_> = response.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["PC-PARIS-A", "PC-PARIS-1"]);

    let response = app
        .get("/api/v2/computers?per_page=1000", Some(&admin))
        .await;
    assert_eq!(response.body["per_page"], 100);

    let response = app
        .get("/api/v2/computers?sort=password", Some(&admin))
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn api_responses_are_not_cached() {
    let app = test_app!();
    let response = app
        .router
        .clone()
        .oneshot(Request::get("/api/v2/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
}
