//! Asset writes on a real database: rights, history in the PHP format,
//! side effects, and the refusals sending a change to the classic interface.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

/// History rows of an item: `(search option, old, new, link, action, author)`.
async fn history(
    app: &common::TestApp,
    itemtype: &str,
    id: i32,
) -> Vec<(i32, String, String, String, i32, String)> {
    sqlx::query_as(
        "SELECT id_search_option, old_value, new_value, itemtype_link, linked_action, user_name \
         FROM glpi_logs WHERE itemtype = ? AND items_id = ? ORDER BY id",
    )
    .bind(itemtype)
    .bind(id)
    .fetch_all(&app.pool)
    .await
    .unwrap()
}

/// Asset events: `(item id, type, service, level, message)`.
async fn events(app: &common::TestApp) -> Vec<(i32, String, String, i32, String)> {
    sqlx::query_as(
        "SELECT items_id, type, service, level, message FROM glpi_events \
         WHERE service = 'inventory' ORDER BY id",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap()
}

async fn column(app: &common::TestApp, sql: &str) -> Option<String> {
    sqlx::query_scalar(sql).fetch_one(&app.pool).await.unwrap()
}

fn row(option: i32, old: &str, new: &str) -> (i32, String, String, String, i32, String) {
    (
        option,
        old.into(),
        new.into(),
        String::new(),
        0,
        "manager (19)".into(),
    )
}

fn item_row(action: i32) -> (i32, String, String, String, i32, String) {
    (
        0,
        String::new(),
        String::new(),
        "0".into(),
        action,
        "manager (19)".into(),
    )
}

impl common::TestApp {
    async fn patch(&self, uri: &str, cookie: &str, body: Value) -> common::Response {
        self.send(Method::PATCH, uri, Some(cookie), Some(body))
            .await
    }

    async fn post(&self, uri: &str, cookie: &str, body: Option<Value>) -> common::Response {
        self.send(Method::POST, uri, Some(cookie), body).await
    }
}

#[tokio::test]
async fn every_asset_type_is_listed() {
    let app = test_app!();
    let manager = app.login("manager").await;
    assert_eq!(
        app.names("/api/v2/assets/monitors", &manager).await,
        ["MON-PARIS-1", "MON-SHARED"]
    );
    for kind in ["printers", "network-equipments", "peripherals", "phones"] {
        let response = app
            .get(&format!("/api/v2/assets/{kind}"), Some(&manager))
            .await;
        assert_eq!(response.status, StatusCode::OK, "{kind}: {}", response.body);
        assert_eq!(response.body["total"], 0);
    }
    let response = app.get("/api/v2/assets/tablets", Some(&manager)).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // The technician profile has no monitor right.
    let paris = app.login("paris").await;
    let response = app.get("/api/v2/assets/monitors", Some(&paris)).await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    let detail = app.get("/api/v2/assets/monitors/201", Some(&manager)).await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(detail.body["kind"], "monitors");
    assert_eq!(detail.body["is_global"], true);
    assert_eq!(detail.body["network"], Value::Null);
    assert_eq!(
        detail.body["permissions"],
        json!({ "update": true, "trash": true })
    );
}

#[tokio::test]
async fn creation_follows_the_rights_and_the_selected_entities() {
    let app = test_app!();
    let body = json!({ "entity_id": 1, "name": "PC <new>", "comment": "a > b", "location_id": 1, "state_id": 1 });

    // Technician: read and update, no create.
    let paris = app.login("paris").await;
    let response = app
        .post("/api/v2/assets/computers", &paris, Some(body.clone()))
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    // The root entity is visible to the manager (shared items) but not
    // selected: nothing can be created there.
    let manager = app.login("manager").await;
    let mut root = body.clone();
    root["entity_id"] = json!(0);
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(root))
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    let response = app
        .post("/api/v2/assets/computers", &manager, Some(body))
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let id = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(response.body["name"], "PC <new>");
    assert_eq!(response.body["comment"], "a > b");
    assert_eq!(response.body["location"]["name"], "Paris HQ > Room 1");
    assert_eq!(response.body["state"]["name"], "In use");
    assert_eq!(response.body["entity"]["id"], 1);
    assert!(response.body["created_at"].is_string());

    // Stored like the PHP code, with one creation entry and one event.
    assert_eq!(
        column(
            &app,
            &format!("SELECT name FROM glpi_computers WHERE id = {id}")
        )
        .await
        .as_deref(),
        Some("PC &lt;new&gt;")
    );
    assert_eq!(
        column(
            &app,
            &format!("SELECT comment FROM glpi_computers WHERE id = {id}")
        )
        .await
        .as_deref(),
        Some("a &gt; b")
    );
    assert_eq!(history(&app, "Computer", id).await, [item_row(20)]);
    assert_eq!(
        events(&app).await,
        [(
            id,
            "computers".into(),
            "inventory".into(),
            4,
            "manager adds the item PC &lt;new&gt;".into()
        )]
    );

    // Searching the decoded text finds the encoded name.
    assert_eq!(
        app.names("/api/v2/assets/computers?search=%3Cnew", &manager)
            .await,
        ["PC <new>"]
    );
}

#[tokio::test]
async fn invalid_fields_are_rejected() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let cases = [
        ("monitors", json!({ "entity_id": 1, "uuid": "x" })),
        ("monitors", json!({ "entity_id": 1, "network_id": 0 })),
        ("computers", json!({ "entity_id": 1, "brand": "x" })),
        ("computers", json!({ "entity_id": 1, "location_id": 999 })),
        ("computers", json!({ "entity_id": 1, "user_id": 999 })),
        ("computers", json!({ "entity_id": 1, "type_id": -1 })),
        (
            "computers",
            json!({ "entity_id": 1, "name": "x".repeat(256) }),
        ),
        ("computers", json!({ "entity_id": 1, "name": 3 })),
        ("computers", json!({ "name": "no entity" })),
    ];
    for (kind, body) in cases {
        let response = app
            .post(
                &format!("/api/v2/assets/{kind}"),
                &manager,
                Some(body.clone()),
            )
            .await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "{kind} {body}: {}",
            response.body
        );
    }
    // 255 characters fit, counted as stored (`<` takes 4).
    let response = app
        .post(
            "/api/v2/assets/peripherals",
            &manager,
            Some(json!({ "entity_id": 1, "name": "é".repeat(255), "brand": "Logi" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let response = app
        .post(
            "/api/v2/assets/peripherals",
            &manager,
            Some(json!({ "entity_id": 1, "name": format!("{}<", "x".repeat(252)) })),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_writes_the_history_like_the_php_code() {
    let app = test_app!();
    let manager = app.login("manager").await;

    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({
                "name": "PC-PARIS-1b",
                "serial": "SN-P1",
                "location_id": null,
                "user_id": 11,
                "state_id": 1,
                "comment": "a < b",
                "tech_group_id": 1,
            }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["name"], "PC-PARIS-1b");
    assert_eq!(response.body["location"], Value::Null);
    assert_eq!(response.body["user"], json!({ "id": 11, "name": "paris" }));
    assert_eq!(response.body["comment"], "a < b");

    // Unchanged serial: no entry.
    assert_eq!(
        history(&app, "Computer", 102).await,
        [
            row(1, "PC-PARIS-1", "PC-PARIS-1b"),
            row(16, "", "a &lt; b"),
            row(3, "Paris HQ &gt; Room 1 (1)", "&nbsp; (0)"),
            row(31, "&nbsp; (0)", "In use (1)"),
            row(70, "admin (10)", "paris (11)"),
            row(49, "&nbsp; (0)", "Support (1)"),
        ]
    );
    assert_eq!(
        events(&app).await,
        [(
            102,
            "computers".into(),
            "inventory".into(),
            4,
            "manager updates an item".into()
        )]
    );

    // Nothing changed: no history and the modification date is kept.
    app.sql("UPDATE glpi_computers SET date_mod = '2026-01-03 10:00:00' WHERE id = 102")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "name": "PC-PARIS-1b", "location_id": 0 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(history(&app, "Computer", 102).await.len(), 6);
    assert_eq!(
        column(
            &app,
            "SELECT CAST(date_mod AS CHAR) FROM glpi_computers WHERE id = 102"
        )
        .await
        .as_deref(),
        Some("2026-01-03 10:00:00")
    );

    // Without propagation settings, connected items do not follow.
    assert_eq!(history(&app, "Monitor", 200).await, []);
}

#[tokio::test]
async fn computer_changes_follow_to_connected_items_and_components() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES \
         ('core', 'is_contact_autoupdate', '1'), ('core', 'is_user_autoupdate', '1'), \
         ('core', 'is_group_autoupdate', '1'), ('core', 'is_location_autoupdate', '1'), \
         ('core', 'state_autoupdate_mode', '-1')",
    )
    .await;
    let manager = app.login("manager").await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "contact": "jdupont", "location_id": 3, "state_id": 1, "serial": "SN-NEW" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);

    // The monitor follows (not the serial number); the global one does not.
    assert_eq!(
        history(&app, "Monitor", 200).await,
        [
            row(7, "old", "jdupont"),
            row(3, "Paris HQ &gt; Room 1 (1)", "Warehouse (3)"),
            row(31, "&nbsp; (0)", "In use (1)"),
        ]
    );
    let monitor = app.get("/api/v2/assets/monitors/200", Some(&manager)).await;
    assert_eq!(monitor.body["contact"], "jdupont");
    assert_eq!(monitor.body["location"]["id"], 3);
    assert_eq!(monitor.body["state"]["id"], 1);
    assert_eq!(history(&app, "Monitor", 201).await, []);
    let shared = app.get("/api/v2/assets/monitors/201", Some(&manager)).await;
    assert_eq!(shared.body["contact"], "old");

    // The processor follows the location and status, with an entry on it
    // and one on the computer.
    assert_eq!(
        history(&app, "Item_DeviceProcessor", 1).await,
        [
            row(13, "Paris HQ &gt; Room 1 (1)", "Warehouse (3)"),
            row(14, "&nbsp; (0)", "In use (1)")
        ]
    );
    let device_entries: Vec<_> = history(&app, "Computer", 102)
        .await
        .into_iter()
        .filter(|entry| entry.4 == 2)
        .collect();
    assert_eq!(
        device_entries,
        [
            (
                0,
                "1".into(),
                "3".into(),
                "Item_DeviceProcessor#locations_id".into(),
                2,
                "manager (19)".into()
            ),
            (
                0,
                "0".into(),
                "1".into(),
                "Item_DeviceProcessor#states_id".into(),
                2,
                "manager (19)".into()
            ),
        ]
    );
    assert_eq!(
        column(&app, "SELECT CONCAT(locations_id, '/', states_id) FROM glpi_items_deviceprocessors WHERE id = 1")
            .await
            .as_deref(),
        Some("3/1")
    );
}

#[tokio::test]
async fn trash_and_restore() {
    let app = test_app!();
    let manager = app.login("manager").await;

    // Technician: no delete right.
    let paris = app.login("paris").await;
    let response = app
        .post("/api/v2/assets/computers/102/trash", &paris, None)
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    let response = app
        .post("/api/v2/assets/computers/102/trash", &manager, None)
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["is_deleted"], true);
    assert_eq!(
        column(&app, "SELECT CAST(is_deleted_item AS CHAR) FROM glpi_items_softwareversions WHERE items_id = 102")
            .await
            .as_deref(),
        Some("1")
    );
    assert!(
        app.names("/api/v2/assets/computers?trash=true", &manager)
            .await
            .contains(&"PC-PARIS-1".to_owned())
    );

    // Already in the trash: no entry, no event.
    let response = app
        .post("/api/v2/assets/computers/102/trash", &manager, None)
        .await;
    assert_eq!(response.status, StatusCode::OK);

    let response = app
        .post("/api/v2/assets/computers/102/restore", &manager, None)
        .await;
    assert_eq!(response.body["is_deleted"], false);
    assert_eq!(
        column(&app, "SELECT CAST(is_deleted_item AS CHAR) FROM glpi_items_softwareversions WHERE items_id = 102")
            .await
            .as_deref(),
        Some("0")
    );
    assert_eq!(
        history(&app, "Computer", 102).await,
        [item_row(13), item_row(14)]
    );
    let messages: Vec<_> = events(&app)
        .await
        .into_iter()
        .map(|event| event.4)
        .collect();
    assert_eq!(
        messages,
        ["manager deletes an item", "manager restores an item"]
    );

    // Items of an inventory tool are locked rather than deleted.
    app.sql("UPDATE glpi_monitors SET is_dynamic = 1 WHERE id = 200")
        .await;
    app.post("/api/v2/assets/monitors/200/trash", &manager, None)
        .await;
    app.post("/api/v2/assets/monitors/200/restore", &manager, None)
        .await;
    assert_eq!(
        history(&app, "Monitor", 200).await,
        [item_row(26), item_row(27)]
    );
}

#[tokio::test]
async fn writes_stay_in_the_selected_entities() {
    let app = test_app!();
    let manager = app.login("manager").await;

    // Shared from the root: visible, read-only.
    let shared = app
        .get("/api/v2/assets/computers/101", Some(&manager))
        .await;
    assert_eq!(shared.status, StatusCode::OK);
    assert_eq!(
        shared.body["permissions"],
        json!({ "update": false, "trash": false })
    );
    let response = app
        .patch(
            "/api/v2/assets/computers/101",
            &manager,
            json!({ "name": "x" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let response = app
        .post("/api/v2/assets/computers/101/trash", &manager, None)
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    // Other entity, template: not found.
    for id in [104, 106] {
        let response = app
            .patch(
                &format!("/api/v2/assets/computers/{id}"),
                &manager,
                json!({ "name": "x" }),
            )
            .await;
        assert_eq!(response.status, StatusCode::NOT_FOUND, "computer {id}");
    }
    let response = app
        .patch(
            "/api/v2/assets/monitors/202",
            &manager,
            json!({ "name": "x" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(history(&app, "Computer", 101).await, []);
}

/// A `RuleAsset` rule of the root entity, run on `condition` (1 add, 2
/// update, 3 both).
async fn rule(
    app: &common::TestApp,
    id: i32,
    condition: i32,
    criteria: &[(&str, i32, &str)],
    actions: &[(&str, &str, &str)],
) {
    let mut sql = format!(
        "INSERT INTO glpi_rules (id, entities_id, sub_type, ranking, name, `match`, is_active, `condition`) \
         VALUES ({id}, 0, 'RuleAsset', {id}, 'Rule {id}', 'AND', 1, {condition});"
    );
    for (criteria, condition, pattern) in criteria {
        sql += &format!(
            "INSERT INTO glpi_rulecriterias (rules_id, criteria, `condition`, pattern) \
             VALUES ({id}, '{criteria}', {condition}, '{pattern}');"
        );
    }
    for (action_type, field, value) in actions {
        sql += &format!(
            "INSERT INTO glpi_ruleactions (rules_id, action_type, field, value) \
             VALUES ({id}, '{action_type}', '{field}', '{value}');"
        );
    }
    app.sql(&sql).await;
}

#[tokio::test]
async fn business_rules_change_the_written_values() {
    let app = test_app!();
    let manager = app.login("manager").await;
    // Update of a computer whose contact is "jdupont": location and comment.
    rule(
        &app,
        1,
        2,
        &[("_itemtype", 0, "Computer"), ("contact", 0, "JDUPONT")],
        &[
            ("assign", "locations_id", "3"),
            ("assign", "comment", "set by rule"),
        ],
    )
    .await;
    // The user comes from an e-mail address in the contact.
    rule(
        &app,
        2,
        3,
        &[("_auto", 0, "0"), ("contact", 6, "/^(.*)@example/")],
        &[("regex_result", "_affect_user_by_regex", "#0")],
    )
    .await;
    // Inventory tools only: never run here.
    rule(
        &app,
        3,
        3,
        &[("_auto", 0, "1")],
        &[("assign", "states_id", "2")],
    )
    .await;
    // Without criteria, inactive, or defined in a sub-entity: never run.
    rule(&app, 4, 3, &[], &[("assign", "states_id", "2")]).await;
    rule(
        &app,
        5,
        3,
        &[("contact", 8, "")],
        &[("assign", "states_id", "2")],
    )
    .await;
    app.sql("UPDATE glpi_rules SET is_active = 0 WHERE id = 5")
        .await;
    rule(
        &app,
        6,
        3,
        &[("contact", 8, "")],
        &[("assign", "states_id", "2")],
    )
    .await;
    app.sql("UPDATE glpi_rules SET entities_id = 1 WHERE id = 6")
        .await;

    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "contact": "jdupont" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["location"]["id"], 3);
    assert_eq!(response.body["comment"], "set by rule");
    assert_eq!(response.body["state"], Value::Null);
    assert_eq!(
        history(&app, "Computer", 102).await,
        [
            row(7, "", "jdupont"),
            row(3, "Paris HQ &gt; Room 1 (1)", "Warehouse (3)"),
            row(16, "", "set by rule"),
        ]
    );

    // Rules see the whole form: an unrelated change runs them again, and
    // what they set stays.
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "comment": "mine", "serial": "S2" }),
        )
        .await;
    assert_eq!(response.body["comment"], "set by rule");
    assert_eq!(response.body["serial"], "S2");

    // Creation with a contact from which the user login is taken.
    let response = app
        .post(
            "/api/v2/assets/monitors",
            &manager,
            Some(json!({ "entity_id": 1, "name": "MON-NEW", "contact": "paris@example.org" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert_eq!(response.body["user"]["id"], 11);
    // Unknown login: no user.
    let response = app
        .post(
            "/api/v2/assets/monitors",
            &manager,
            Some(json!({ "entity_id": 1, "name": "MON-NEW-2", "contact": "nobody@example.org" })),
        )
        .await;
    assert_eq!(response.body["user"], Value::Null);
}

#[tokio::test]
async fn connected_items_run_their_own_rules() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES \
         ('core', 'is_location_autoupdate', '1'), ('core', 'is_contact_autoupdate', '1')",
    )
    .await;
    // Monitors moved to the warehouse get a status; the rules see the
    // copied fields (location, contact).
    rule(
        &app,
        1,
        2,
        &[("_itemtype", 0, "Monitor"), ("locations_id", 0, "3")],
        &[("assign", "states_id", "1"), ("assign", "comment", "moved")],
    )
    .await;
    rule(
        &app,
        2,
        2,
        &[("_itemtype", 0, "Monitor"), ("contact", 2, "jd")],
        &[("assign", "users_id", "13")],
    )
    .await;
    let manager = app.login("manager").await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "location_id": 3, "contact": "jd" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    // Not on the computer itself.
    assert_eq!(response.body["state"], Value::Null);

    let monitor = app.get("/api/v2/assets/monitors/200", Some(&manager)).await;
    assert_eq!(monitor.body["location"]["id"], 3);
    assert_eq!(monitor.body["state"]["id"], 1);
    assert_eq!(monitor.body["comment"], "moved");
    assert_eq!(monitor.body["user"]["id"], 13);
    // The global monitor neither follows nor runs the rules.
    let shared = app.get("/api/v2/assets/monitors/201", Some(&manager)).await;
    assert_eq!(shared.body["state"], Value::Null);
}

#[tokio::test]
async fn infocoms_are_created_and_dated_like_php() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let infocom = |itemtype: &'static str, id: i32| {
        let pool = app.pool.clone();
        async move {
            sqlx::query_as::<_, (i32, i32, bool, i32, Option<String>, Option<String>, Option<String>)>(
                "SELECT id, entities_id, is_recursive, alert, CAST(buy_date AS CHAR), CAST(use_date AS CHAR), \
                 CAST(warranty_date AS CHAR) FROM glpi_infocoms WHERE itemtype = ? AND items_id = ?",
            )
            .bind(itemtype)
            .bind(id)
            .fetch_optional(&pool)
            .await
            .unwrap()
        }
    };
    let today: String = sqlx::query_scalar("SELECT CAST(CURDATE() AS CHAR)")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Created with the item; default alert of the root entity, inherited.
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'auto_create_infocoms', '1'); \
         UPDATE glpi_entities SET default_infocom_alert = 4 WHERE id = 0",
    )
    .await;
    let response = app
        .post(
            "/api/v2/assets/printers",
            &manager,
            Some(json!({ "entity_id": 1, "name": "PRN-1", "is_recursive": true })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let printer = response.body["id"].as_i64().unwrap() as i32;
    let (infocom_id, entity, recursive, alert, buy, _, _) =
        infocom("Printer", printer).await.unwrap();
    assert_eq!((entity, recursive, alert, buy), (1, true, 4, None));
    assert_eq!(history(&app, "Infocom", infocom_id).await, [item_row(20)]);

    // Dates filled by the status, set on the root entity and inherited.
    app.sql(
        "UPDATE glpi_entities SET autofill_buy_date = 'STATUS_1', autofill_use_date = 'STATUS_1', \
         autofill_warranty_date = 'STATUS_2' WHERE id = 0",
    )
    .await;
    let response = app
        .patch(
            &format!("/api/v2/assets/printers/{printer}"),
            &manager,
            json!({ "state_id": 1 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    let (_, _, _, _, buy, used, warranty) = infocom("Printer", printer).await.unwrap();
    assert_eq!(
        (buy.as_deref(), used.as_deref(), warranty),
        (Some(today.as_str()), Some(today.as_str()), None)
    );
    // Logged on the printer with the time, as the PHP code does.
    let dates: Vec<_> = history(&app, "Printer", printer)
        .await
        .into_iter()
        .filter(|entry| matches!(entry.0, 37 | 38))
        .collect();
    assert_eq!(dates.len(), 2);
    assert_eq!((dates[0].0, dates[1].0), (37, 38));
    assert_eq!(dates[0].1, "");
    assert!(
        dates[0].2.starts_with(&today) && dates[0].2.len() == 19,
        "{}",
        dates[0].2
    );

    // Back to the status the same day: rewritten, but not logged again.
    for state in [2, 1] {
        app.patch(
            &format!("/api/v2/assets/printers/{printer}"),
            &manager,
            json!({ "state_id": state }),
        )
        .await;
    }
    let logged = history(&app, "Printer", printer)
        .await
        .into_iter()
        .filter(|entry| matches!(entry.0, 37 | 38 | 123))
        .count();
    // Status 2 filled the warranty date once.
    assert_eq!(logged, 3);

    // A new item with a status: the infocom is created, then dated.
    app.sql("DELETE FROM glpi_configs WHERE name = 'auto_create_infocoms'")
        .await;
    let response = app
        .post(
            "/api/v2/assets/phones",
            &manager,
            Some(json!({ "entity_id": 1, "name": "PHONE-1", "state_id": 1 })),
        )
        .await;
    let phone = response.body["id"].as_i64().unwrap() as i32;
    let (phone_infocom, _, _, alert, buy, used, _) = infocom("Phone", phone).await.unwrap();
    assert_eq!(
        (alert, buy.as_deref(), used.as_deref()),
        (4, Some(today.as_str()), Some(today.as_str()))
    );
    assert_eq!(
        history(&app, "Infocom", phone_infocom).await,
        [item_row(20)]
    );
    assert!(
        history(&app, "Phone", phone)
            .await
            .iter()
            .all(|entry| entry.0 == 0)
    );
    // Without status nor automatic creation: no infocom.
    let response = app
        .post(
            "/api/v2/assets/phones",
            &manager,
            Some(json!({ "entity_id": 1, "name": "PHONE-2" })),
        )
        .await;
    assert!(
        infocom("Phone", response.body["id"].as_i64().unwrap() as i32)
            .await
            .is_none()
    );

    // A connected monitor getting the status of its computer is dated too.
    app.sql("INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'state_autoupdate_mode', '-1')")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "state_id": 1 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert!(infocom("Computer", 102).await.is_some());
    assert!(infocom("Monitor", 200).await.is_some());
    assert!(infocom("Monitor", 201).await.is_none());
}

#[tokio::test]
async fn sharing_follows_to_the_linked_items() {
    let app = test_app!();
    let manager = app.login("manager").await;
    // PC-PARIS-1 also has an installed software (no sharing flag).
    app.sql(
        "INSERT INTO glpi_infocoms (itemtype, items_id, entities_id, is_recursive) VALUES ('Computer', 102, 1, 0); \
         INSERT INTO glpi_networkports (itemtype, items_id, entities_id, is_recursive, date_mod) \
           VALUES ('Computer', 102, 1, 0, '2026-01-01 00:00:00'); \
         INSERT INTO glpi_reservationitems (itemtype, items_id, entities_id, is_recursive) VALUES ('Computer', 102, 1, 0); \
         INSERT INTO glpi_items_operatingsystems (itemtype, items_id, entities_id, is_recursive, date_mod) \
           VALUES ('Computer', 102, 1, 0, '2026-01-01 00:00:00')",
    )
    .await;
    let flags = || async {
        let mut flags = Vec::new();
        for table in [
            "glpi_infocoms",
            "glpi_networkports",
            "glpi_reservationitems",
            "glpi_items_operatingsystems",
        ] {
            let flag: bool = sqlx::query_scalar(&format!(
                "SELECT is_recursive FROM {table} WHERE itemtype = 'Computer' AND items_id = 102"
            ))
            .fetch_one(&app.pool)
            .await
            .unwrap();
            flags.push(flag);
        }
        flags
    };

    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": true }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(flags().await, [true; 4]);
    let port_date: Option<String> = sqlx::query_scalar(
        "SELECT CAST(date_mod AS CHAR) FROM glpi_networkports WHERE itemtype = 'Computer' AND items_id = 102",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_ne!(port_date.as_deref(), Some("2026-01-01 00:00:00"));
    // Only the change of the computer is logged.
    let logged: Vec<_> = history(&app, "Computer", 102)
        .await
        .into_iter()
        .map(|entry| entry.0)
        .collect();
    assert_eq!(logged, [86]);

    // Unshared: nothing of another entity uses it.
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": false }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(flags().await, [false; 4]);
}

#[tokio::test]
async fn unsharing_checks_the_items_of_other_entities() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let refused = |response: &common::Response| {
        assert_eq!(response.status, StatusCode::CONFLICT, "{}", response.body);
        assert_eq!(response.error_code(), "sharing_in_use");
    };

    // A software installed from Lyon, which sees PC-PARIS-1 only shared.
    app.sql(
        "UPDATE glpi_computers SET is_recursive = 1 WHERE id = 102; \
         INSERT INTO glpi_items_softwareversions (items_id, itemtype, softwareversions_id, entities_id) \
           VALUES (102, 'Computer', 2, 2)",
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": false }),
        )
        .await;
    refused(&response);
    // Without it, the computer can be unshared.
    app.sql("DELETE FROM glpi_items_softwareversions WHERE entities_id = 2")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": false }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);

    // A shared printer wired to a computer of Lyon.
    app.sql(
        "INSERT INTO glpi_printers (id, name, entities_id, is_recursive) VALUES (300, 'PRN', 1, 1); \
         INSERT INTO glpi_networkports (id, itemtype, items_id, entities_id, is_recursive) VALUES \
           (10, 'Printer', 300, 1, 1), (11, 'Computer', 104, 2, 0); \
         INSERT INTO glpi_networkports_networkports (networkports_id_1, networkports_id_2) VALUES (11, 10)",
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/printers/300",
            &manager,
            json!({ "is_recursive": false }),
        )
        .await;
    refused(&response);
    assert_eq!(history(&app, "Printer", 300).await, []);
}

#[tokio::test]
async fn events_are_in_the_language_of_the_session() {
    let app = test_app!();
    app.sql(
        "UPDATE glpi_users SET language = 'fr_FR', is_ids_visible = 1 WHERE name = 'manager'; \
         INSERT INTO glpi_fieldunicities (name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
         VALUES ('Serial', 'Computer', 0, 1, 'serial,contact', 1, 1); \
         UPDATE glpi_computers SET serial = 'SN-FR', contact = 'ops' WHERE id IN (102, 105); \
         UPDATE glpi_computers SET name = 'Poste é' WHERE id = 102; \
         UPDATE glpi_computers SET name = '' WHERE id = 105",
    )
    .await;
    let manager = app.login("manager").await;

    let refused = app
        .post(
            "/api/v2/assets/computers",
            &manager,
            Some(json!({ "entity_id": 1, "name": "NEW", "serial": "SN-FR", "contact": "ops" })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body);
    let created = app
        .post(
            "/api/v2/assets/computers",
            &manager,
            Some(json!({ "entity_id": 1, "name": "Poste <FR>", "serial": "SN-FR2" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let uri = format!("/api/v2/assets/computers/{}", created.body["id"]);
    app.patch(&uri, &manager, json!({ "contact": "x" })).await;
    app.post(&format!("{uri}/trash"), &manager, None).await;
    app.post(&format!("{uri}/restore"), &manager, None).await;

    let messages: Vec<String> = events(&app)
        .await
        .into_iter()
        .map(|event| event.4)
        .collect();
    assert_eq!(
        messages,
        [
            "manager essaie d'ajouter un élément qui existe déjà : Enregistrement impossible pour \
             Numéro de série = SN-FR&nbsp;&amp;&nbsp;Usager = ops<br>Un autre élément existe\
             <br>[<a  href='/front/computer.form.php?id=102'  title=\"Poste &eacute;\">Poste é (102)</a> \
             - ID : 102 - Numéro de série : SN-FR - Entité : Root &gt; Paris]\
             <br>[<a  href='/front/computer.form.php?id=105' >N/A (105)</a> \
             - ID : 105 - Numéro de série : SN-FR - Entité : Root &gt; Paris - Objet à la corbeille]",
            "manager ajoute l'élément Poste &lt;FR&gt;",
            "manager met à jour un élément",
            "manager supprime un élément",
            "manager restaure un élément",
        ]
    );
}

#[tokio::test]
async fn field_unicity_like_php() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let duplicate = |response: &common::Response| {
        assert_eq!(response.status, StatusCode::CONFLICT, "{}", response.body);
        assert_eq!(response.error_code(), "duplicate");
        assert_eq!(response.body["error"]["details"], json!(["serial"]));
    };
    let new = |serial: &str| Some(json!({ "entity_id": 1, "name": "NEW", "serial": serial }));
    app.sql(
        "INSERT INTO glpi_fieldunicities (id, name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
         VALUES (1, 'Serial', 'Computer', 0, 1, 'serial', 1, 1)",
    )
    .await;

    // Refused, with the event of the PHP code.
    duplicate(
        &app.post("/api/v2/assets/computers", &manager, new("SN-P1"))
            .await,
    );
    assert_eq!(
        events(&app).await,
        [(
            0,
            "Computer".into(),
            "inventory".into(),
            4,
            "manager trying to add an item that already exists: Impossible record for Serial number = SN-P1\
             <br>Other item exist<br>[<a  href='/front/computer.form.php?id=102'  title=\"PC-PARIS-1\">PC-PARIS-1</a> \
             - ID: 102 - Serial number: SN-P1 - Entity: Root &gt; Paris]"
                .into()
        )]
    );
    // Compared by the database: case and trailing spaces do not count.
    duplicate(
        &app.post("/api/v2/assets/computers", &manager, new("sn-p1 "))
            .await,
    );
    // Items in the trash count, templates do not.
    app.sql("UPDATE glpi_computers SET serial = 'SN-T' WHERE id = 105; UPDATE glpi_computers SET serial = 'SN-TPL' WHERE id = 106")
        .await;
    duplicate(
        &app.post("/api/v2/assets/computers", &manager, new("SN-T"))
            .await,
    );
    let response = app
        .post("/api/v2/assets/computers", &manager, new("SN-TPL"))
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    // Empty values and blacklisted ones are not compared.
    let response = app
        .post("/api/v2/assets/computers", &manager, new(""))
        .await;
    assert_eq!(response.status, StatusCode::CREATED);
    app.sql(
        "INSERT INTO glpi_fieldblacklists (name, field, value, itemtype, entities_id, is_recursive) \
         VALUES ('OEM', 'serial', 'SN-P1', 'Computer', 0, 1)",
    )
    .await;
    let response = app
        .post("/api/v2/assets/computers", &manager, new("SN-P1"))
        .await;
    assert_eq!(response.status, StatusCode::CREATED);
    app.sql("DELETE FROM glpi_fieldblacklists").await;

    // The whole form is checked: another field of a duplicate is refused.
    app.sql("UPDATE glpi_computers SET serial = 'SN-P1' WHERE id = 107")
        .await;
    duplicate(
        &app.patch(
            "/api/v2/assets/computers/107",
            &manager,
            json!({ "contact": "x" }),
        )
        .await,
    );

    // Reported only: written, with the duplicates in the answer.
    app.sql("UPDATE glpi_fieldunicities SET action_refuse = 0")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/107",
            &manager,
            json!({ "contact": "x" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["contact"], "x");
    let items: Vec<i64> = response.body["duplicates"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_i64().unwrap())
        .collect();
    assert!(items.contains(&102), "{}", response.body);
    let last = events(&app).await.into_iter().rev().nth(1).unwrap();
    assert_eq!((last.0, last.1.as_str()), (107, "Computer"));
    assert!(
        last.4
            .contains("Item successfully added but duplicate record on Serial number = SN-P1"),
        "{}",
        last.4
    );

    // The criteria of the nearest entity win: Paris only checks the
    // inventory number, Office A (not covered by Paris) the serial number.
    app.sql(
        "UPDATE glpi_fieldunicities SET action_refuse = 1; \
         INSERT INTO glpi_fieldunicities (name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
         VALUES ('Inventory', 'Computer', 1, 0, 'otherserial', 1, 1)",
    )
    .await;
    let response = app
        .post("/api/v2/assets/computers", &manager, new("SN-P1"))
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let office = Some(json!({ "entity_id": 3, "name": "NEW", "serial": "SN-P1" }));
    duplicate(&app.post("/api/v2/assets/computers", &manager, office).await);
    app.sql("DELETE FROM glpi_fieldunicities WHERE id <> 1")
        .await;

    // A criterion sending notifications: left to the classic interface
    // when they are on, without event.
    app.sql(
        "UPDATE glpi_fieldunicities SET action_notify = 1; \
         INSERT INTO glpi_configs (context, name, value) VALUES \
           ('core', 'use_notifications', '1'), ('core', 'notifications_mailing', '1'); \
         INSERT INTO glpi_notifications (name, entities_id, itemtype, event, is_recursive, is_active) \
           VALUES ('Unicity', 0, 'FieldUnicity', 'refuse', 1, 1)",
    )
    .await;
    let before = events(&app).await.len();
    let response = app
        .post("/api/v2/assets/computers", &manager, new("SN-P1"))
        .await;
    assert_eq!(response.error_code(), "legacy_required");
    assert_eq!(response.body["error"]["details"], json!(["notifications"]));
    assert_eq!(events(&app).await.len(), before);
    app.sql("UPDATE glpi_configs SET value = '0' WHERE name = 'use_notifications'")
        .await;
    duplicate(
        &app.post("/api/v2/assets/computers", &manager, new("SN-P1"))
            .await,
    );
}

#[tokio::test]
async fn connected_items_refused_by_unicity_do_not_follow() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'is_contact_autoupdate', '1'); \
         INSERT INTO glpi_monitors (id, name, entities_id, contact) VALUES (203, 'MON-JD', 1, 'jd'); \
         INSERT INTO glpi_fieldunicities (name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
         VALUES ('Contact', 'Monitor', 0, 1, 'contact', 1, 1)",
    )
    .await;
    let manager = app.login("manager").await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "contact": "jd" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["contact"], "jd");
    let monitor = app.get("/api/v2/assets/monitors/200", Some(&manager)).await;
    assert_eq!(monitor.body["contact"], "old");
    let logged: Vec<_> = events(&app)
        .await
        .into_iter()
        .map(|event| (event.0, event.1))
        .collect();
    assert_eq!(
        logged,
        [(200, "Monitor".to_owned()), (102, "computers".to_owned())]
    );
}

#[tokio::test]
async fn unported_php_features_send_the_change_to_the_classic_interface() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let refused = |response: &common::Response, reason: &str| {
        assert_eq!(response.status, StatusCode::CONFLICT, "{}", response.body);
        assert_eq!(response.error_code(), "legacy_required");
        assert_eq!(response.body["error"]["details"], json!([reason]));
    };
    let create = json!({ "entity_id": 1, "name": "new" });

    // Rules the engine cannot run identically: a plugin criterion, a
    // regular expression only PCRE understands.
    rule(
        &app,
        1,
        2,
        &[("_plugin_tag", 0, "x")],
        &[("assign", "comment", "x")],
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "name": "x" }),
        )
        .await;
    refused(&response, "business_rules");
    // A rule meant for updates also runs on creations, as in PHP.
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(create.clone()))
        .await;
    refused(&response, "business_rules");
    app.sql("DELETE FROM glpi_rulecriterias; DELETE FROM glpi_ruleactions; DELETE FROM glpi_rules")
        .await;
    rule(
        &app,
        2,
        2,
        &[("contact", 6, "/(a)\\\\1/")],
        &[("assign", "comment", "x")],
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "serial": "y" }),
        )
        .await;
    refused(&response, "business_rules");
    app.sql("DELETE FROM glpi_rulecriterias; DELETE FROM glpi_ruleactions; DELETE FROM glpi_rules")
        .await;

    // Unicity of the infocoms written along.
    app.sql(
        "INSERT INTO glpi_fieldunicities (name, itemtype, entities_id, fields, is_active, action_refuse) \
         VALUES ('Dates', 'Infocom', 0, 'buy_date', 1, 1); \
         INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'auto_create_infocoms', '1')",
    )
    .await;
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(create.clone()))
        .await;
    refused(&response, "field_unicity");
    app.sql("DELETE FROM glpi_fieldunicities; DELETE FROM glpi_configs WHERE name = 'auto_create_infocoms'")
        .await;

    // A unicity criterion on a field the form does not have.
    app.sql(
        "INSERT INTO glpi_fieldunicities (name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
         VALUES ('Size', 'Monitor', 0, 1, 'size', 1, 1)",
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/monitors/200",
            &manager,
            json!({ "name": "x" }),
        )
        .await;
    refused(&response, "field_unicity");
    app.sql("DELETE FROM glpi_fieldunicities").await;

    // Hooks of the PHP plugins.
    app.sql(
        "INSERT INTO glpi_plugins (directory, name, version, state) VALUES \
         ('fields', 'Fields', '1.0', 1), ('old', 'Old', '1.0', 4)",
    )
    .await;
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(create.clone()))
        .await;
    refused(&response, "plugins");
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "serial": "w" }),
        )
        .await;
    refused(&response, "plugins");
    let response = app
        .post("/api/v2/assets/computers/102/trash", &manager, None)
        .await;
    refused(&response, "plugins");
    // Nothing to do: nothing refused.
    let response = app
        .post("/api/v2/assets/computers/102/restore", &manager, None)
        .await;
    assert_eq!(response.status, StatusCode::OK);
    app.sql("UPDATE glpi_plugins SET state = 4").await;

    // Sharing an item whose linked items are in another entity.
    app.sql(
        "INSERT INTO glpi_networkports (itemtype, items_id, entities_id, is_recursive) VALUES ('Computer', 102, 2, 0)",
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": true }),
        )
        .await;
    refused(&response, "linked_items");

    // Refusals write nothing.
    assert_eq!(history(&app, "Computer", 102).await, []);
}

#[tokio::test]
async fn choices_are_those_of_the_legacy_forms() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let labels = |response: common::Response| -> Vec<String> {
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        response.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap().to_owned())
            .collect()
    };
    let choices = |path: &str| format!("/api/v2/assets/{path}");

    // Own values and recursive ones of the parents, not those of Lyon.
    let response = app
        .get(
            &choices("computers/choices/location?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Paris HQ > Room 1", "Warehouse"]);
    let response = app
        .get(
            &choices("computers/choices/location?entity_id=1&search=room"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Paris HQ > Room 1"]);

    // Statuses offered for the type.
    let response = app
        .get(
            &choices("computers/choices/state?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["In use"]);
    let response = app
        .get(
            &choices("monitors/choices/state?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["In use", "Stock"]);

    // Groups: of items, in charge of items.
    let response = app
        .get(
            &choices("computers/choices/group?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Owners", "Support"]);
    let response = app
        .get(
            &choices("computers/choices/tech_group?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Support"]);
    let response = app
        .get(
            &choices("computers/choices/group?entity_id=3"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Support"]);

    // Users with a profile in the entity; technicians own tickets.
    let response = app
        .get(
            &choices("computers/choices/user?entity_id=1"),
            Some(&manager),
        )
        .await;
    let users = labels(response);
    assert!(users.contains(&"Admin Alice".to_owned()), "{users:?}");
    assert!(users.contains(&"paris".to_owned()), "{users:?}");
    for absent in ["lyon", "disabled", "expired", "noprofile"] {
        assert!(!users.contains(&absent.to_owned()), "{absent} in {users:?}");
    }
    let response = app
        .get(
            &choices("computers/choices/tech_user?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["manager"]);

    let response = app
        .get(
            &choices("computers/choices/manufacturer?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Dell"]);
    let response = app
        .get(
            &choices("computers/choices/type?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(labels(response), ["Laptop"]);

    // Unknown field for the type, entity not selected, no write right.
    let response = app
        .get(
            &choices("monitors/choices/network?entity_id=1"),
            Some(&manager),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let response = app
        .get(&choices("computers/choices/location"), Some(&manager))
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let response = app
        .get(
            &choices("computers/choices/location?entity_id=2"),
            Some(&manager),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let helpdesk = app.login("helpdesk").await;
    let response = app
        .get(
            &choices("computers/choices/location?entity_id=0"),
            Some(&helpdesk),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    // Update right is enough.
    let paris = app.login("paris").await;
    let response = app
        .get(
            &choices("computers/choices/location?entity_id=1"),
            Some(&paris),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
}
