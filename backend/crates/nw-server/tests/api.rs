//! API tests against a real MariaDB/MySQL server.
//!
//! Each test creates its own database from `install/mysql/glpi-empty.sql`
//! and `tests/fixtures/seed.sql`, then drops it. They need
//! `NW_TEST_DATABASE_URL`, a server URL **without** database name whose
//! user may create databases, e.g. `mysql://root@127.0.0.1:3306`.
//! Without it the tests are skipped.

use std::time::Duration;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use nw_server::{config::Config, state::AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

const SCHEMA: &str = include_str!("../../../../install/mysql/glpi-empty.sql");
const SEED: &str = include_str!("fixtures/seed.sql");
const SECRET: &str = "test-secret-test-secret-test-secret";

struct TestApp {
    router: Router,
    pool: nw_db::MySqlPool,
    _database: TestDatabase,
}

/// Drops the database even when the test panics.
struct TestDatabase {
    server_url: String,
    name: String,
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let url = self.server_url.clone();
        let name = self.name.clone();
        // Drop cannot be async: use a dedicated runtime on another thread.
        let _ = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                if let Ok(pool) = sqlx::MySqlPool::connect(&url).await {
                    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS `{name}`"))
                        .execute(&pool)
                        .await;
                }
            });
        })
        .join();
    }
}

async fn app() -> Option<TestApp> {
    let Ok(server_url) = std::env::var("NW_TEST_DATABASE_URL") else {
        eprintln!("skipped: NW_TEST_DATABASE_URL is not set");
        return None;
    };
    let server_url = server_url.trim_end_matches('/').to_owned();
    let name = format!("nw_test_{:08x}", rand::random::<u32>());

    let server = sqlx::MySqlPool::connect(&server_url)
        .await
        .expect("database server reachable");
    sqlx::query(&format!(
        "CREATE DATABASE `{name}` CHARACTER SET utf8 COLLATE utf8_unicode_ci"
    ))
    .execute(&server)
    .await
    .expect("create test database");
    server.close().await;
    let database = TestDatabase {
        server_url: server_url.clone(),
        name: name.clone(),
    };

    let pool = nw_db::connect(&format!("{server_url}/{name}"), 5)
        .await
        .expect("connect");
    sqlx::raw_sql(SCHEMA)
        .execute(&pool)
        .await
        .expect("load schema");
    sqlx::raw_sql(SEED)
        .execute(&pool)
        .await
        .expect("load fixtures");

    let config = Config {
        database_url: String::new(),
        database_max_connections: 5,
        bind_address: ([127, 0, 0, 1], 0).into(),
        session_secret: SECRET.as_bytes().to_vec(),
        session_ttl: Duration::from_secs(3600),
        cookie_secure: true,
        static_dir: None,
    };
    let router = nw_server::router(AppState::new(pool.clone(), &config), &config);
    Some(TestApp {
        router,
        pool,
        _database: database,
    })
}

struct Response {
    status: StatusCode,
    set_cookie: Option<String>,
    body: Value,
}

impl TestApp {
    async fn request(&self, request: Request<Body>) -> Response {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|value| value.to_str().unwrap().to_owned());
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        Response {
            status,
            set_cookie,
            body,
        }
    }

    async fn login_response(&self, login: &str, password: &str) -> Response {
        self.request(
            Request::post("/api/v2/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "login": login, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
    }

    /// Logs in and returns the `nw_session=...` cookie pair.
    async fn login(&self, login: &str) -> String {
        let response = self.login_response(login, "secret").await;
        assert_eq!(
            response.status,
            StatusCode::OK,
            "login {login}: {}",
            response.body
        );
        let cookie = response.set_cookie.expect("session cookie");
        cookie.split(';').next().unwrap().to_owned()
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Response {
        let mut request = Request::get(uri);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        self.request(request.body(Body::empty()).unwrap()).await
    }

    /// Names of the computers listed by `uri` for a session.
    async fn names(&self, uri: &str, cookie: &str) -> Vec<String> {
        let response = self.get(uri, Some(cookie)).await;
        assert_eq!(response.status, StatusCode::OK, "{uri}: {}", response.body);
        response.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

macro_rules! test_app {
    () => {
        match app().await {
            Some(app) => app,
            None => return,
        }
    };
}

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
    let cookie = response.set_cookie.unwrap();
    assert!(cookie.starts_with("nw_session="));
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
    sqlx::raw_sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES \
         ('core', 'password_expiration_delay', '30'), \
         ('core', 'password_expiration_lock_delay', '10');
         UPDATE glpi_users SET password_last_update = NOW() WHERE name = 'admin';
         UPDATE glpi_users SET password_last_update = NOW() - INTERVAL 35 DAY WHERE name = 'paris';",
    )
    .execute(&app.pool)
    .await
    .unwrap();

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
    sqlx::query("UPDATE glpi_users SET is_active = 0 WHERE name = 'admin'")
        .execute(&app.pool)
        .await
        .unwrap();
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
    let cookie = response.set_cookie.unwrap();
    assert!(cookie.starts_with("nw_session=;"), "{cookie}");
    assert!(cookie.contains("Max-Age=0"), "{cookie}");
}

#[tokio::test]
async fn computers_follow_the_entity_scope() {
    let app = test_app!();

    // Root, recursive: everything but the trash and templates.
    let admin = app.login("admin").await;
    let mut names = app.names("/api/v2/computers?per_page=100", &admin).await;
    names.sort();
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
    let mut names = app.names("/api/v2/computers?per_page=100", &paris).await;
    names.sort();
    assert_eq!(
        names,
        ["100%_PC", "PC-PARIS-1", "PC-PARIS-A", "PC-ROOT-SHARED"]
    );

    // Lyon, not recursive.
    let lyon = app.login("lyon").await;
    let mut names = app.names("/api/v2/computers?per_page=100", &lyon).await;
    names.sort();
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
