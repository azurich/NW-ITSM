//! Users (`glpi_users`).

use chrono::{DateTime, Utc};
use nw_domain::history::{self, Change, Entry};
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
    pub auths_id: i32,
    pub is_active: bool,
    pub is_deleted: bool,
    pub begin_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub profiles_id: i32,
    pub entities_id: i32,
    /// Display preferences; `None` means the configuration default.
    pub names_format: Option<i32>,
    pub is_ids_visible: Option<bool>,
}

impl User {
    /// Account stored in the ITSM-NG database (not LDAP, mail or external).
    pub fn is_local(&self) -> bool {
        self.authtype == AUTH_TYPE_LOCAL && self.auths_id == 0
    }

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
    authtype, auths_id, is_active, is_deleted, begin_date, end_date, profiles_id, entities_id, \
    names_format, is_ids_visible";

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

/// Records a successful login (`last_login`, not part of the history).
pub async fn record_login(pool: &MySqlPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE glpi_users SET last_login = NOW() WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Replaces an obsolete hash of the same password. Not a password change:
/// the expiration date and the modification date are kept.
pub async fn upgrade_password_hash(
    pool: &MySqlPool,
    id: i32,
    old_hash: &str,
    new_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE glpi_users SET password = ? WHERE id = ? AND password = ?")
        .bind(new_hash)
        .bind(id)
        .bind(old_hash)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Sets a new password, like `User::prepareInputForUpdate`: the
/// expiration delay starts again. The password has no history entry.
pub async fn change_password(pool: &MySqlPool, id: i32, new_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE glpi_users SET password = ?, password_last_update = NOW(), date_mod = NOW() \
         WHERE id = ?",
    )
    .bind(new_hash)
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Changes the preferred language (`None`: configuration default) and
/// records it in the history. Returns `false` when nothing changed.
pub async fn change_language(
    pool: &MySqlPool,
    id: i32,
    language: Option<&str>,
    actor: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let current: Option<(Option<String>,)> =
        sqlx::query_as("SELECT language FROM glpi_users WHERE id = ? FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((current,)) = current else {
        return Ok(false);
    };
    if current.as_deref() == language {
        return Ok(false);
    }
    sqlx::query("UPDATE glpi_users SET language = ?, date_mod = NOW() WHERE id = ?")
        .bind(language)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let change = Change::new(
        history::user::LANGUAGE,
        current.as_deref().unwrap_or_default(),
        language.unwrap_or_default(),
    );
    let entry = Entry::field(history::user::ITEMTYPE, id, change);
    crate::history::record(&mut tx, actor, &entry).await?;
    tx.commit().await?;
    Ok(true)
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
            auths_id: 0,
            is_active: true,
            is_deleted: false,
            begin_date: None,
            end_date: None,
            profiles_id: 0,
            entities_id: 0,
            names_format: None,
            is_ids_visible: None,
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
