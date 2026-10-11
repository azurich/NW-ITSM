//! Test harness: an API on a fresh database.
//!
//! Each test creates its own database from `install/mysql/glpi-empty.sql`
//! and `tests/fixtures/seed.sql`, then drops it. The tests need
//! `NW_TEST_DATABASE_URL`, a server URL **without** database name whose
//! user may create databases, e.g. `mysql://root@127.0.0.1:3306`.
//! Without it they are skipped.

#![allow(dead_code)] // each test file uses part of the harness

use std::time::Duration;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use nw_server::{config::Config, state::AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

const SCHEMA: &str = include_str!("../../../../../install/mysql/glpi-empty.sql");
const SEED: &str = include_str!("../fixtures/seed.sql");
pub const SESSION_SECRET: &str = "test-secret-test-secret-test-secret";
pub const BRIDGE_SECRET: &str = "test-bridge-secret-test-bridge-secret";

pub struct TestApp {
    pub router: Router,
    pub pool: nw_db::MySqlPool,
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

/// A new application, or `None` when no test database server is configured.
pub async fn app() -> Option<TestApp> {
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
        session_secret: SESSION_SECRET.as_bytes().to_vec(),
        session_ttl: Duration::from_secs(3600),
        cookie_secure: true,
        static_dir: None,
        trust_proxy: false,
        bridge_secret: Some(BRIDGE_SECRET.as_bytes().to_vec()),
    };
    let router = nw_server::router(AppState::new(pool.clone(), &config), &config);
    Some(TestApp {
        router,
        pool,
        _database: database,
    })
}

#[macro_export]
macro_rules! test_app {
    () => {
        match common::app().await {
            Some(app) => app,
            None => return,
        }
    };
}

pub struct Response {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    /// Every `Set-Cookie` header.
    pub cookies: Vec<String>,
    pub body: Value,
}

impl Response {
    /// `name=value` pair of a cookie set by the response.
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.cookies
            .iter()
            .map(|cookie| cookie.split(';').next().unwrap())
            .find(|pair| pair.starts_with(&format!("{name}=")))
            .map(str::to_owned)
    }

    /// The full `Set-Cookie` header of a cookie.
    pub fn set_cookie(&self, name: &str) -> Option<&str> {
        self.cookies
            .iter()
            .find(|cookie| cookie.starts_with(&format!("{name}=")))
            .map(String::as_str)
    }

    pub fn error_code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or_default()
    }
}

impl TestApp {
    pub async fn request(&self, request: Request<Body>) -> Response {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let cookies = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().to_owned())
            .collect();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        Response {
            status,
            headers,
            cookies,
            body,
        }
    }

    /// Sends a request with an optional `Cookie` header and JSON body.
    pub async fn send(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<Value>,
    ) -> Response {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let body = match body {
            Some(body) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(body.to_string())
            }
            None => Body::empty(),
        };
        self.request(request.body(body).unwrap()).await
    }

    pub async fn get(&self, uri: &str, cookie: Option<&str>) -> Response {
        self.send(Method::GET, uri, cookie, None).await
    }

    pub async fn login_response(&self, login: &str, password: &str) -> Response {
        self.send(
            Method::POST,
            "/api/v2/auth/login",
            None,
            Some(json!({ "login": login, "password": password })),
        )
        .await
    }

    /// Logs in with the fixture password and returns the session cookie pair.
    pub async fn login(&self, login: &str) -> String {
        let response = self.login_response(login, "secret").await;
        assert_eq!(
            response.status,
            StatusCode::OK,
            "login {login}: {}",
            response.body
        );
        response.cookie("nw_session").expect("session cookie")
    }

    /// Names of the computers listed by `uri` for a session.
    pub async fn names(&self, uri: &str, cookie: &str) -> Vec<String> {
        let response = self.get(uri, Some(cookie)).await;
        assert_eq!(response.status, StatusCode::OK, "{uri}: {}", response.body);
        let mut names: Vec<String> = response.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_owned())
            .collect();
        names.sort();
        names
    }

    pub async fn sql(&self, sql: &str) {
        sqlx::raw_sql(sql).execute(&self.pool).await.unwrap();
    }
}
