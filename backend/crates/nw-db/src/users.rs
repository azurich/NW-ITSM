//! Users (`glpi_users`).

use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySqlPool};

/// Legacy `Auth::DB_GLPI`: account stored in the ITSM-NG database.
pub const AUTH_TYPE_LOCAL: i32 = 1;

/// The user columns needed to authenticate and describe a session.
#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: i32,
    pub name: Option<String>,
    pub password: Option<String>,
    pub password_last_update: Option<DateTime<Utc>>,
    pub realname: Option<String>,
    pub firstname: Option<String>,
    pub language: Option<String>,
    pub authtype: i32,
    pub is_active: bool,
    pub is_deleted: bool,
    pub begin_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub profiles_id: i32,
    pub entities_id: i32,
}

impl User {
    /// Same check as legacy `Session::init`: the account is active, not in
    /// the trash, and inside its optional validity period.
    pub fn is_allowed_at(&self, now: DateTime<Utc>) -> bool {
        !self.is_deleted
            && self.is_active
            && self.begin_date.is_none_or(|begin| begin < now)
            && self.end_date.is_none_or(|end| end > now)
    }
}

const COLUMNS: &str = "id, name, password, password_last_update, realname, firstname, language, \
    authtype, is_active, is_deleted, begin_date, end_date, profiles_id, entities_id";

/// Finds a local account (not LDAP, mail or external) by login.
///
/// Like the legacy `Auth::connection_db`, an ambiguous login matches nobody.
pub async fn find_local_by_login(
    pool: &MySqlPool,
    login: &str,
) -> Result<Option<User>, sqlx::Error> {
    let mut users: Vec<User> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM glpi_users WHERE name = ? AND authtype = ? AND auths_id = 0"
    ))
    .bind(login)
    .bind(AUTH_TYPE_LOCAL)
    .fetch_all(pool)
    .await?;
    Ok(if users.len() == 1 { users.pop() } else { None })
}

/// Finds a user by id.
pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM glpi_users WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    fn user() -> User {
        User {
            id: 2,
            name: Some("itsm".to_owned()),
            password: None,
            password_last_update: None,
            realname: None,
            firstname: None,
            language: None,
            authtype: AUTH_TYPE_LOCAL,
            is_active: true,
            is_deleted: false,
            begin_date: None,
            end_date: None,
            profiles_id: 0,
            entities_id: 0,
        }
    }

    #[test]
    fn validity_period() {
        let now = Utc::now();
        let day = TimeDelta::days(1);
        assert!(user().is_allowed_at(now));
        assert!(
            !User {
                is_active: false,
                ..user()
            }
            .is_allowed_at(now)
        );
        assert!(
            !User {
                is_deleted: true,
                ..user()
            }
            .is_allowed_at(now)
        );
        assert!(
            !User {
                begin_date: Some(now + day),
                ..user()
            }
            .is_allowed_at(now)
        );
        assert!(
            !User {
                end_date: Some(now - day),
                ..user()
            }
            .is_allowed_at(now)
        );
        assert!(
            User {
                begin_date: Some(now - day),
                end_date: Some(now + day),
                ..user()
            }
            .is_allowed_at(now)
        );
    }
}
