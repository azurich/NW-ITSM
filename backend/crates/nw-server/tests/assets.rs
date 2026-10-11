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
        "SELECT id_search_option, old_value, IFNULL(new_value, 'NULL'), itemtype_link, \
         linked_action, user_name FROM glpi_logs WHERE itemtype = ? AND items_id = ? ORDER BY id",
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
        json!({ "update": true, "trash": true, "purge": true })
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

    // Unchanged serial: no entry. In the order of the legacy form.
    assert_eq!(
        history(&app, "Computer", 102).await,
        [
            row(1, "PC-PARIS-1", "PC-PARIS-1b"),
            row(31, "&nbsp; (0)", "In use (1)"),
            row(3, "Paris HQ &gt; Room 1 (1)", "&nbsp; (0)"),
            row(49, "&nbsp; (0)", "Support (1)"),
            row(70, "admin (10)", "paris (11)"),
            row(16, "", "a &lt; b"),
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

    // The monitor follows (not the serial number), in the order of the
    // computer's changes; the global one does not.
    assert_eq!(
        history(&app, "Monitor", 200).await,
        [
            row(31, "&nbsp; (0)", "In use (1)"),
            row(3, "Paris HQ &gt; Room 1 (1)", "Warehouse (3)"),
            row(7, "old", "jdupont"),
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
            row(14, "&nbsp; (0)", "In use (1)"),
            row(13, "Paris HQ &gt; Room 1 (1)", "Warehouse (3)")
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
                "0".into(),
                "1".into(),
                "Item_DeviceProcessor#states_id".into(),
                2,
                "manager (19)".into()
            ),
            (
                0,
                "1".into(),
                "3".into(),
                "Item_DeviceProcessor#locations_id".into(),
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
        json!({ "update": false, "trash": false, "purge": false })
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
    // Creations only: not run by the updates.
    rule(
        &app,
        7,
        1,
        &[("_itemtype", 0, "Computer"), ("contact", 0, "JDUPONT")],
        &[("assign", "states_id", "1")],
    )
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
            row(3, "Paris HQ &gt; Room 1 (1)", "Warehouse (3)"),
            row(7, "", "jdupont"),
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

    // A creation runs the rules of creations, not those of updates.
    let response = app
        .post(
            "/api/v2/assets/computers",
            &manager,
            Some(json!({ "entity_id": 1, "name": "PC-NEW", "contact": "jdupont" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert_eq!(response.body["state"]["id"], 1);
    assert_eq!(response.body["location"], Value::Null);
    assert_eq!(response.body["comment"], "");
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
async fn fields_of_each_type_like_php() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_printers (id, name, entities_id, date_mod) VALUES (300, 'PRN', 1, '2026-01-01'); \
         INSERT INTO glpi_networkequipments (id, name, entities_id, ram, date_mod) VALUES (400, 'SW', 1, '512', '2026-01-01'); \
         INSERT INTO glpi_phones (id, name, entities_id, date_mod) VALUES (500, 'TEL', 1, '2026-01-01'); \
         INSERT INTO glpi_phonepowersupplies (id, name) VALUES (1, 'PoE')",
    )
    .await;
    let manager = app.login("manager").await;
    let patch = |uri: &'static str, body: Value| {
        let manager = manager.clone();
        let app = &app;
        async move { app.patch(uri, &manager, body).await }
    };

    // Monitor: size filtered like the legacy form, flags, global
    // management, contact.
    let monitor = patch(
        "/api/v2/assets/monitors/200",
        json!({ "size": "21,50", "has_hdmi": true, "has_speaker": true, "is_global": true, "contact": "x" }),
    )
    .await;
    assert_eq!(monitor.status, StatusCode::OK, "{}", monitor.body);
    assert_eq!(monitor.body["size"], "21.50");
    assert_eq!(monitor.body["has_hdmi"], true);
    assert_eq!(monitor.body["has_dvi"], false);
    assert_eq!(monitor.body["has_wifi"], Value::Null);
    assert_eq!(monitor.body["is_global"], true);
    // The same number written otherwise is no change.
    patch("/api/v2/assets/monitors/200", json!({ "size": "21.5" })).await;
    assert_eq!(
        history(&app, "Monitor", 200).await,
        [
            row(7, "old", "x"),
            row(11, "0.00", "21.5"),
            row(82, "0", "1"),
            row(42, "0", "1"),
            row(47, "0", "1"),
        ]
    );
    for (size, error) in [
        ("abc", "size: a decimal number is required"),
        ("", "size: a decimal number is required"),
        ("1e2", "size: a decimal number is required"),
        ("999.995", "size: out of range"),
    ] {
        let response = patch("/api/v2/assets/monitors/200", json!({ "size": size })).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{size}");
        assert_eq!(response.body["error"]["message"], error, "{size}");
    }
    let response = patch("/api/v2/assets/monitors/200", json!({ "has_wifi": true })).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // Printer: ports, memory, page counters, comment.
    let printer = patch(
        "/api/v2/assets/printers/300",
        json!({ "has_wifi": true, "memory": "64 Mo <max>", "initial_page_counter": 10, "page_counter": 250, "comment": "c" }),
    )
    .await;
    assert_eq!(printer.status, StatusCode::OK, "{}", printer.body);
    assert_eq!(printer.body["memory"], "64 Mo <max>");
    assert_eq!(printer.body["page_counter"], 250);
    assert_eq!(printer.body["has_wifi"], true);
    assert_eq!(printer.body["has_hdmi"], Value::Null);
    assert_eq!(
        history(&app, "Printer", 300).await,
        [
            row(13, "", "64 Mo &lt;max&gt;"),
            row(11, "0", "10"),
            row(12, "0", "250"),
            row(16, "", "c"),
            row(46, "0", "1"),
        ]
    );

    // Network equipment: memory in MiB, compared as a number.
    patch(
        "/api/v2/assets/network-equipments/400",
        json!({ "memory": "0512" }),
    )
    .await;
    let network = patch(
        "/api/v2/assets/network-equipments/400",
        json!({ "memory": " 1024 " }),
    )
    .await;
    assert_eq!(network.body["memory"], "1024");
    let response = patch(
        "/api/v2/assets/network-equipments/400",
        json!({ "memory": "1.5" }),
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        history(&app, "NetworkEquipment", 400).await,
        [row(14, "512", "1024")]
    );

    // Phone: power supply, headset, speaker.
    let phone = patch(
        "/api/v2/assets/phones/500",
        json!({ "power_supply_id": 1, "has_speaker": true, "has_headset": true }),
    )
    .await;
    assert_eq!(phone.status, StatusCode::OK, "{}", phone.body);
    assert_eq!(phone.body["power_supply"]["name"], "PoE");
    assert_eq!(phone.body["has_speaker"], true);
    assert_eq!(
        history(&app, "Phone", 500).await,
        [
            row(42, "&nbsp; (0)", "PoE (1)"),
            row(43, "0", "1"),
            row(44, "0", "1")
        ]
    );
    let choices = app
        .get(
            "/api/v2/assets/phones/choices/power_supply?entity_id=1",
            Some(&manager),
        )
        .await;
    assert_eq!(choices.body["items"], json!([{ "id": 1, "label": "PoE" }]));

    // Creation: an empty size keeps its default, the counters are 0.
    let created = app
        .post(
            "/api/v2/assets/monitors",
            &manager,
            Some(json!({ "entity_id": 1, "name": "NEW" })),
        )
        .await;
    assert_eq!(created.body["size"], "0.00");
    let created = app
        .post(
            "/api/v2/assets/printers",
            &manager,
            Some(json!({ "entity_id": 1, "name": "NEW", "initial_page_counter": 5 })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["initial_page_counter"], 5);
    assert_eq!(created.body["page_counter"], 0);
}

async fn count(app: &common::TestApp, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&app.pool).await.unwrap()
}

#[tokio::test]
async fn purge_like_php() {
    let app = test_app!();
    let manager = app.login("manager").await;
    let purge = |uri: &'static str| {
        let manager = manager.clone();
        let app = &app;
        async move { app.send(Method::DELETE, uri, Some(&manager), None).await }
    };
    // A monitor connected to computer 102, with linked rows the PHP code
    // deletes with it, and some of its history.
    app.sql(
        "INSERT INTO glpi_infocoms (id, itemtype, items_id, entities_id, value) VALUES (7, 'Monitor', 200, 1, 10); \
         INSERT INTO glpi_alerts (itemtype, items_id, type) VALUES ('Infocom', 7, 2); \
         INSERT INTO glpi_logs (itemtype, items_id, itemtype_link, linked_action, user_name, date_mod, id_search_option, old_value, new_value) VALUES \
           ('Infocom', 7, '', 20, 'x', NOW(), 0, '', ''), ('Monitor', 200, '', 20, 'x', NOW(), 0, '', ''); \
         INSERT INTO glpi_contracts_items (contracts_id, itemtype, items_id) VALUES (1, 'Monitor', 200); \
         INSERT INTO glpi_documents_items (documents_id, itemtype, items_id) VALUES (1, 'Monitor', 200), (1, 'Monitor', 201); \
         INSERT INTO glpi_notepads (itemtype, items_id, content) VALUES ('Monitor', 200, 'n'); \
         INSERT INTO glpi_reservationitems (id, itemtype, items_id, entities_id) VALUES (5, 'Monitor', 200, 1); \
         INSERT INTO glpi_items_operatingsystems (itemtype, items_id, operatingsystems_id) VALUES ('Monitor', 200, 1); \
         INSERT INTO glpi_domains_items (domains_id, itemtype, items_id) VALUES (1, 'Monitor', 200)",
    )
    .await;

    // From the trash only, and purging its infocom needs that right too.
    assert_eq!(
        purge("/api/v2/assets/monitors/200").await.error_code(),
        "not_in_trash"
    );
    app.post("/api/v2/assets/monitors/200/trash", &manager, None)
        .await;
    assert_eq!(
        purge("/api/v2/assets/monitors/200").await.status,
        StatusCode::FORBIDDEN
    );
    app.sql("INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'infocom', 31)")
        .await;

    // Linked rows whose purge is not ported.
    for sql in [
        "INSERT INTO glpi_networkports (id, itemtype, items_id, entities_id) VALUES (9, 'Monitor', 200, 1)",
        "INSERT INTO glpi_items_devicecases (id, itemtype, items_id, devicecases_id) VALUES (9, 'Monitor', 200, 1)",
        "INSERT INTO glpi_items_tickets (id, itemtype, items_id, tickets_id) VALUES (9, 'Monitor', 200, 1)",
        "INSERT INTO glpi_reservations (id, reservationitems_id, users_id) VALUES (9, 5, 19)",
        "INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'is_location_autoclean', '1')",
    ] {
        app.sql(sql).await;
        let response = purge("/api/v2/assets/monitors/200").await;
        assert_eq!(response.error_code(), "legacy_required", "{sql}");
        assert_eq!(response.body["error"]["details"], json!(["linked_data"]));
        app.sql(
            "DELETE FROM glpi_networkports WHERE id = 9; DELETE FROM glpi_items_devicecases WHERE id = 9; \
             DELETE FROM glpi_items_tickets WHERE id = 9; DELETE FROM glpi_reservations WHERE id = 9; \
             DELETE FROM glpi_configs WHERE name = 'is_location_autoclean'",
        )
        .await;
    }

    let response = purge("/api/v2/assets/monitors/200").await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        app.get("/api/v2/assets/monitors/200", Some(&manager))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    for (table, left) in [
        ("glpi_monitors WHERE id = 200", 0),
        ("glpi_infocoms", 0),
        ("glpi_alerts", 0),
        (
            "glpi_logs WHERE (itemtype = 'Monitor' AND items_id = 200) OR itemtype = 'Infocom'",
            0,
        ),
        ("glpi_computers_items WHERE items_id = 200", 0),
        ("glpi_contracts_items", 0),
        // Another monitor keeps its document.
        ("glpi_documents_items", 1),
        ("glpi_notepads", 0),
        ("glpi_reservationitems", 0),
        ("glpi_items_operatingsystems", 0),
        ("glpi_domains_items", 0),
        // The computer keeps its other connection.
        ("glpi_computers_items WHERE computers_id = 102", 1),
    ] {
        assert_eq!(
            count(&app, &format!("SELECT COUNT(*) FROM {table}")).await,
            left,
            "{table}"
        );
    }
    let last = events(&app).await.pop().unwrap();
    assert_eq!(
        last,
        (
            200,
            "monitors".into(),
            "inventory".into(),
            4,
            "manager purges an item".into()
        )
    );

    // A computer: its disks, antivirus and virtual machines, with their
    // history.
    app.sql(
        "UPDATE glpi_computers SET is_deleted = 1 WHERE id = 107; \
         INSERT INTO glpi_items_disks (id, itemtype, items_id, entities_id) VALUES (3, 'Computer', 107, 1); \
         INSERT INTO glpi_computerantiviruses (id, computers_id) VALUES (4, 107); \
         INSERT INTO glpi_computervirtualmachines (id, computers_id, entities_id) VALUES (5, 107, 1); \
         INSERT INTO glpi_logs (itemtype, items_id, itemtype_link, linked_action, user_name, date_mod, id_search_option, old_value, new_value) VALUES \
           ('Item_Disk', 3, '', 20, 'x', NOW(), 0, '', ''), ('ComputerAntivirus', 4, '', 20, 'x', NOW(), 0, '', '')",
    )
    .await;
    let response = purge("/api/v2/assets/computers/107").await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        count(
            &app,
            "SELECT (SELECT COUNT(*) FROM glpi_items_disks) + (SELECT COUNT(*) FROM glpi_computerantiviruses) \
             + (SELECT COUNT(*) FROM glpi_computervirtualmachines) \
             + (SELECT COUNT(*) FROM glpi_logs WHERE itemtype IN ('Item_Disk', 'ComputerAntivirus'))"
        )
        .await,
        0
    );
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
    // Refused, the update still logs the event of the legacy form, after
    // the one of the duplicate.
    let logged: Vec<(i32, String)> = events(&app)
        .await
        .into_iter()
        .rev()
        .take(2)
        .map(|event| (event.0, event.4))
        .collect();
    assert_eq!(logged[0], (107, "manager updates an item".to_owned()));
    assert!(
        logged[1]
            .1
            .contains("Impossible record for Serial number = SN-P1"),
        "{}",
        logged[1].1
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
    // A rule meant for updates does not run on creations.
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(create.clone()))
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
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
         VALUES ('TCO', 'Monitor', 0, 1, 'ticket_tco', 1, 1)",
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

    // The size is in the form of a monitor, except on a creation without
    // size (the PHP code drops it): the criterion is then skipped.
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
    assert_eq!(response.error_code(), "duplicate", "{}", response.body);
    let response = app
        .post("/api/v2/assets/monitors", &manager, Some(create.clone()))
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
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

#[tokio::test]
async fn templates_are_managed_like_php() {
    let app = test_app!();
    let manager = app.login("manager").await;
    app.sql(
        "UPDATE glpi_computers SET template_name = 'Laptop' WHERE id = 106; \
         INSERT INTO glpi_computers (id, name, entities_id, is_template, template_name) VALUES \
           (108, 'LY', 2, 1, 'Lyon'), (109, NULL, 1, 1, 'Desktop'); \
         INSERT INTO glpi_fieldunicities (id, name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
           VALUES (1, 'Name', 'Computer', 0, 1, 'name', 1, 1); \
         INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'auto_create_infocoms', '1'); \
         UPDATE glpi_entities SET autofill_buy_date = 'STATUS_1' WHERE id = 0",
    )
    .await;

    // The templates of the selected entities, by name; not among the items.
    let response = app
        .get("/api/v2/assets/computers/templates", Some(&manager))
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["can_create"], json!(true));
    let names: Vec<&str> = response.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["template_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Desktop", "Laptop"]);
    assert_eq!(
        app.get("/api/v2/assets/computers/106", Some(&manager))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let response = app
        .get("/api/v2/assets/computers/templates/106", Some(&manager))
        .await;
    assert_eq!(response.body["is_template"], json!(true));
    assert_eq!(response.body["template_name"], json!("Laptop"));
    assert_eq!(
        response.body["permissions"],
        json!({ "update": true, "trash": false, "purge": true })
    );
    assert_eq!(
        app.get("/api/v2/assets/computers/templates/102", Some(&manager))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let helpdesk = app.login("helpdesk").await;
    assert_eq!(
        app.get("/api/v2/assets/computers/templates", Some(&helpdesk))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // A new template: its name is not checked for unicity, its status
    // fills no date, it gets an infocom like an item.
    let response = app
        .post(
            "/api/v2/assets/computers/templates",
            &manager,
            Some(json!({
                "entity_id": 1, "template_name": "Server", "name": "PC-PARIS-1", "state_id": 1
            })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert_eq!(response.body["is_template"], json!(true));
    assert_eq!(response.body["template_name"], json!("Server"));
    let id = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(history(&app, "Computer", id).await, [item_row(20)]);
    assert_eq!(
        column(
            &app,
            &format!(
                "SELECT CAST(buy_date AS CHAR) FROM glpi_infocoms WHERE itemtype = 'Computer' AND items_id = {id}"
            )
        )
        .await,
        None
    );
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        "manager adds the item PC-PARIS-1"
    );
    // Not in a parent entity.
    assert_eq!(
        app.post(
            "/api/v2/assets/computers/templates",
            &manager,
            Some(json!({ "entity_id": 0, "template_name": "Root" })),
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // Changed like an item, its name first in the history; still not
    // checked for unicity.
    let response = app
        .patch(
            &format!("/api/v2/assets/computers/templates/{id}"),
            &manager,
            json!({ "name": "PC-PARIS-A", "template_name": "Big server" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["template_name"], json!("Big server"));
    assert_eq!(
        history(&app, "Computer", id).await[1..],
        [
            row(65, "Server", "Big server"),
            row(1, "PC-PARIS-1", "PC-PARIS-A")
        ]
    );
    // Not through the routes of the items, nor the other way round.
    assert_eq!(
        app.patch(
            &format!("/api/v2/assets/computers/{id}"),
            &manager,
            json!({ "name": "X" })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.post(
            &format!("/api/v2/assets/computers/{id}/trash"),
            &manager,
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.patch(
            "/api/v2/assets/computers/templates/102",
            &manager,
            json!({ "name": "X" })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // Purged without the trash, with its infocom (and that right).
    let uri = format!("/api/v2/assets/computers/templates/{id}");
    assert_eq!(
        app.send(Method::DELETE, &uri, Some(&manager), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    app.sql("INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'infocom', 31)")
        .await;
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        count(
            &app,
            &format!(
                "SELECT (SELECT COUNT(*) FROM glpi_computers WHERE id = {id}) \
                 + (SELECT COUNT(*) FROM glpi_infocoms WHERE items_id = {id}) \
                 + (SELECT COUNT(*) FROM glpi_logs WHERE itemtype = 'Computer' AND items_id = {id})"
            )
        )
        .await,
        0
    );
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        "manager purges an item"
    );
}

#[tokio::test]
async fn items_are_created_from_templates_like_php() {
    let app = test_app!();
    let manager = app.login("manager").await;
    // A template of Paris with values the form does not show, an infocom,
    // contracts and documents.
    app.sql(
        "UPDATE glpi_computers SET template_name = 'Laptop', name = 'TPL', serial = NULL, comment = 'model', \
           locations_id = 1, computertypes_id = 1, is_recursive = 1, is_dynamic = 1, ticket_tco = 12.5 \
         WHERE id = 106; \
         INSERT INTO glpi_infocoms (id, itemtype, items_id, entities_id, value, buy_date, alert, comment) \
           VALUES (7, 'Computer', 106, 1, 100, '2026-02-01', 1, 'bought'); \
         INSERT INTO glpi_contracts (id, name, entities_id, max_links_allowed) VALUES (1, 'Support', 1, 0), (2, 'Full', 1, 1); \
         INSERT INTO glpi_contracts_items (contracts_id, itemtype, items_id) VALUES (1, 'Computer', 106), (2, 'Computer', 106); \
         INSERT INTO glpi_documents (id, name, entities_id) VALUES (1, '', 1); \
         INSERT INTO glpi_documents_items (documents_id, itemtype, items_id, entities_id, users_id, date) \
           VALUES (1, 'Computer', 106, 1, 10, '2026-01-01 08:00:00'); \
         UPDATE glpi_entities SET default_infocom_alert = 4 WHERE id = 1",
    )
    .await;

    let response = app
        .post(
            "/api/v2/assets/computers/templates/106/items",
            &manager,
            Some(json!({ "name": "PC-NEW", "serial": "SN-NEW" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let id = response.body["id"].as_i64().unwrap() as i32;
    // The values given, else those of the template; not a template.
    assert_eq!(response.body["is_template"], json!(false));
    assert_eq!(response.body["template_name"], Value::Null);
    assert_eq!(response.body["name"], json!("PC-NEW"));
    assert_eq!(response.body["serial"], json!("SN-NEW"));
    assert_eq!(response.body["comment"], json!("model"));
    assert_eq!(response.body["entity"]["id"], json!(1));
    assert_eq!(response.body["is_recursive"], json!(true));
    assert_eq!(response.body["location"]["id"], json!(1));
    assert_eq!(
        column(
            &app,
            &format!(
                "SELECT CAST(CONCAT_WS('|', is_dynamic, ticket_tco, is_template, IFNULL(template_name, '-'), uuid) AS CHAR) \
                 FROM glpi_computers WHERE id = {id}"
            )
        )
        .await
        .as_deref(),
        // An empty text of the template is posted empty.
        Some("1|12.5000|0|-|")
    );
    // Its infocom, with the default alert of the entity.
    assert_eq!(
        column(
            &app,
            &format!(
                "SELECT CAST(CONCAT_WS('|', entities_id, is_recursive, value, buy_date, alert, comment) AS CHAR) \
                 FROM glpi_infocoms WHERE itemtype = 'Computer' AND items_id = {id}"
            )
        )
        .await
        .as_deref(),
        Some("1|1|100.0000|2026-02-01|4|bought")
    );
    let infocom: i32 = sqlx::query_scalar(&format!(
        "SELECT id FROM glpi_infocoms WHERE itemtype = 'Computer' AND items_id = {id}"
    ))
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(history(&app, "Infocom", infocom).await, [item_row(20)]);
    // Its contracts, except the one having all its items, and documents,
    // with the history of both sides.
    assert_eq!(
        count(
            &app,
            &format!("SELECT COUNT(*) FROM glpi_contracts_items WHERE items_id = {id}")
        )
        .await,
        1
    );
    assert_eq!(
        column(
            &app,
            &format!(
                "SELECT CAST(CONCAT_WS('|', entities_id, is_recursive, users_id, date) AS CHAR) \
                 FROM glpi_documents_items WHERE items_id = {id}"
            )
        )
        .await
        .as_deref(),
        Some("1|1|10|2026-01-01 08:00:00")
    );
    let link = |itemtype: &str, name: &str| {
        (
            0,
            String::new(),
            name.to_owned(),
            itemtype.to_owned(),
            15,
            "manager (19)".to_owned(),
        )
    };
    assert_eq!(
        history(&app, "Computer", id).await,
        [
            item_row(20),
            link("Contract", "Support (1)"),
            link("Document", "N/A (1)")
        ]
    );
    assert_eq!(
        history(&app, "Contract", 1).await,
        [link("Computer", &format!("PC-NEW ({id})"))]
    );
    assert_eq!(
        history(&app, "Document", 1).await,
        [link("Computer", &format!("PC-NEW ({id})"))]
    );
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            id,
            "computers".into(),
            "inventory".into(),
            4,
            "manager adds the item PC-NEW".into()
        )
    );

    // An item dated by its status keeps its own infocom.
    app.sql("UPDATE glpi_entities SET autofill_buy_date = 'STATUS_1' WHERE id = 0")
        .await;
    let response = app
        .post(
            "/api/v2/assets/computers/templates/106/items",
            &manager,
            Some(json!({ "name": "PC-NEW-2", "state_id": 1 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let second = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(
        column(
            &app,
            &format!(
                "SELECT CAST(CONCAT_WS('|', COUNT(*), MAX(value), MAX(buy_date) = CURDATE()) AS CHAR) \
                 FROM glpi_infocoms WHERE itemtype = 'Computer' AND items_id = {second}"
            )
        )
        .await
        .as_deref(),
        Some("1|0.0000|1")
    );

    // The unicity of the fields applies, on every column of the template.
    app.sql(
        "INSERT INTO glpi_fieldunicities (id, name, itemtype, entities_id, is_recursive, fields, is_active, action_refuse) \
         VALUES (1, 'TCO', 'Computer', 0, 1, 'ticket_tco,name', 1, 1)",
    )
    .await;
    let response = app
        .post(
            "/api/v2/assets/computers/templates/106/items",
            &manager,
            Some(json!({ "name": "PC-NEW" })),
        )
        .await;
    assert_eq!(response.error_code(), "duplicate", "{}", response.body);
    app.sql("DELETE FROM glpi_fieldunicities").await;

    // Linked data whose cloning is not ported.
    for sql in [
        "INSERT INTO glpi_networkports (id, itemtype, items_id, entities_id) VALUES (9, 'Computer', 106, 1)",
        "INSERT INTO glpi_items_disks (id, itemtype, items_id, entities_id) VALUES (9, 'Computer', 106, 1)",
        "INSERT INTO glpi_computers_items (id, computers_id, itemtype, items_id) VALUES (9, 106, 'Monitor', 202)",
    ] {
        app.sql(sql).await;
        let response = app
            .post(
                "/api/v2/assets/computers/templates/106/items",
                &manager,
                Some(json!({ "name": "PC-X" })),
            )
            .await;
        assert_eq!(response.error_code(), "legacy_required", "{sql}");
        assert_eq!(response.body["error"]["details"], json!(["linked_data"]));
        app.sql(
            "DELETE FROM glpi_networkports WHERE id = 9; DELETE FROM glpi_items_disks WHERE id = 9; \
             DELETE FROM glpi_computers_items WHERE id = 9",
        )
        .await;
    }

    // In the entity of the template, one of the session.
    app.sql("UPDATE glpi_computers SET entities_id = 0 WHERE id = 106")
        .await;
    assert_eq!(
        app.post(
            "/api/v2/assets/computers/templates/106/items",
            &manager,
            Some(json!({ "name": "PC-X" })),
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    // An item is not a template.
    assert_eq!(
        app.post(
            "/api/v2/assets/computers/templates/102/items",
            &manager,
            Some(json!({ "name": "PC-X" })),
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn datacenter_items_like_php() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'datacenter', 31), (4, 'cluster', 31); \
         INSERT INTO glpi_dcrooms (id, name, entities_id, is_recursive, vis_cols, vis_rows) \
         VALUES (1, 'Room A', 1, 0, 2, 2), (2, 'Root room', 0, 1, 1, 1); \
         INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'auto_create_infocoms', '1')",
    )
    .await;
    // Business rules are for the inventory assets only.
    rule(&app, 9, 3, &[], &[("assign", "comment", "by rule")]).await;
    let manager = app.login("manager").await;
    let column = |sql: String| {
        let app = &app;
        async move { column(app, &sql).await }
    };

    // A rack as the classic form creates it: its defaults, an infocom, no
    // rule, the event of the rack forms.
    let response = app
        .post(
            "/api/v2/assets/racks",
            &manager,
            Some(json!({ "entity_id": 1, "name": "RACK-1" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let rack = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(response.body["kind"], json!("racks"));
    assert_eq!(response.body["number_units"], json!(42));
    assert_eq!(response.body["contact"], Value::Null);
    assert_eq!(
        column(format!(
            "SELECT CAST(CONCAT_WS('|', number_units, room_orientation, position, bgcolor, width, \
             max_power, IFNULL(comment, '-')) AS CHAR) FROM glpi_racks WHERE id = {rack}"
        ))
        .await
        .as_deref(),
        Some("42|1|0|#F5B7B1|0|0|")
    );
    // Racks do not give their entity to their infocom: it is in the root
    // entity, not shared.
    assert_eq!(
        column(format!(
            "SELECT CAST(CONCAT_WS('|', entities_id, is_recursive) AS CHAR) FROM glpi_infocoms \
             WHERE itemtype = 'Rack' AND items_id = {rack}"
        ))
        .await
        .as_deref(),
        Some("0|0")
    );
    assert_eq!(history(&app, "Rack", rack).await, [item_row(20)]);
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            rack,
            "racks".into(),
            "inventory".into(),
            4,
            "manager adds the item RACK-1".into()
        )
    );

    // In a room: a place another rack outside the trash takes is refused.
    let response = app
        .patch(
            &format!("/api/v2/assets/racks/{rack}"),
            &manager,
            json!({ "room_id": 1, "position": "1,1", "width": 600, "number_units": 48 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["room"]["name"], json!("Room A"));
    // The width is not in the history, the room and the units are.
    assert_eq!(
        history(&app, "Rack", rack).await[1..],
        [row(7, "&nbsp; (0)", "Room A (1)"), row(8, "42", "48")]
    );
    let other = app
        .post(
            "/api/v2/assets/racks",
            &manager,
            Some(json!({ "entity_id": 1, "name": "RACK-2", "room_id": 1, "position": "1,1" })),
        )
        .await;
    assert_eq!(other.error_code(), "position_unavailable", "{}", other.body);
    let response = app
        .get("/api/v2/assets/racks/positions?room_id=1", Some(&manager))
        .await;
    let free: Vec<&str> = response.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["position"].as_str().unwrap())
        .collect();
    assert_eq!(free, ["1,2", "2,1", "2,2"]);
    assert_eq!(response.body["items"][1]["column"], json!("B"));
    // The rooms offered: those of the entity of the rack, and the shared
    // ones of its parents.
    let response = app
        .get(
            "/api/v2/assets/racks/choices/room?entity_id=1",
            Some(&manager),
        )
        .await;
    assert_eq!(
        response.body["items"],
        json!([{ "id": 1, "label": "Room A" }, { "id": 2, "label": "Root room" }])
    );
    // Refused, the update still logs the event of the form.
    let second = app
        .post(
            "/api/v2/assets/racks",
            &manager,
            Some(json!({ "entity_id": 1, "name": "RACK-2", "room_id": 1, "position": "2,2" })),
        )
        .await;
    let second = second.body["id"].as_i64().unwrap() as i32;
    let response = app
        .patch(
            &format!("/api/v2/assets/racks/{second}"),
            &manager,
            json!({ "position": "1,1" }),
        )
        .await;
    assert_eq!(response.error_code(), "position_unavailable");
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        "manager updates an item"
    );
    // A rack in the trash leaves its place.
    app.post(
        &format!("/api/v2/assets/racks/{rack}/trash"),
        &manager,
        None,
    )
    .await;
    let response = app
        .patch(
            &format!("/api/v2/assets/racks/{second}"),
            &manager,
            json!({ "position": "1,1" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    // Validated like the form offers them.
    for body in [
        json!({ "room_orientation": 5 }),
        json!({ "number_units": 0 }),
        json!({ "background_color": "red" }),
        json!({ "width": -1 }),
        json!({ "power_supplies": 2 }),
    ] {
        let response = app
            .patch(
                &format!("/api/v2/assets/racks/{second}"),
                &manager,
                body.clone(),
            )
            .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{body}");
    }
    // Emptied, a number is stored as 0.
    app.patch(
        &format!("/api/v2/assets/racks/{second}"),
        &manager,
        json!({ "max_weight": 300 }),
    )
    .await;
    let response = app
        .patch(
            &format!("/api/v2/assets/racks/{second}"),
            &manager,
            json!({ "max_weight": null }),
        )
        .await;
    assert_eq!(response.body["max_weight"], json!(0));

    // A model left NULL: setting none is a change for PHP.
    app.sql(&format!(
        "UPDATE glpi_racks SET rackmodels_id = NULL WHERE id = {second}"
    ))
    .await;
    let response = app
        .patch(
            &format!("/api/v2/assets/racks/{second}"),
            &manager,
            json!({ "model_id": 0 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(
        history(&app, "Rack", second).await.pop().unwrap(),
        row(40, "&nbsp; ()", "&nbsp; (0)")
    );

    // Enclosures: one power supply by default, no type.
    let response = app
        .post(
            "/api/v2/assets/enclosures",
            &manager,
            Some(json!({ "entity_id": 1, "name": "ENC-1" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert_eq!(response.body["power_supplies"], json!(1));
    assert_eq!(
        app.post(
            "/api/v2/assets/enclosures",
            &manager,
            Some(json!({ "entity_id": 1, "type_id": 1 })),
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // PDUs and passive equipment: comments not in the history.
    let response = app
        .post(
            "/api/v2/assets/pdus",
            &manager,
            Some(json!({ "entity_id": 1, "name": "PDU-1" })),
        )
        .await;
    let pdu = response.body["id"].as_i64().unwrap() as i32;
    let response = app
        .patch(
            &format!("/api/v2/assets/pdus/{pdu}"),
            &manager,
            json!({ "comment": "c", "name": "PDU-2" }),
        )
        .await;
    assert_eq!(response.body["comment"], json!("c"));
    assert_eq!(
        history(&app, "PDU", pdu).await[1..],
        [row(1, "PDU-1", "PDU-2")]
    );
    assert_eq!(events(&app).await.pop().unwrap().1, "pdus".to_owned());

    // Clusters: no template, no infocom, version and type not logged.
    let response = app
        .post(
            "/api/v2/assets/clusters",
            &manager,
            Some(json!({ "entity_id": 1, "name": "CLU-1", "uuid": "u-1" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let cluster = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(
        count(
            &app,
            &format!("SELECT COUNT(*) FROM glpi_infocoms WHERE itemtype = 'Cluster' AND items_id = {cluster}")
        )
        .await,
        0
    );
    app.patch(
        &format!("/api/v2/assets/clusters/{cluster}"),
        &manager,
        json!({ "version": "2", "uuid": "u-2" }),
    )
    .await;
    assert_eq!(
        history(&app, "Cluster", cluster).await[1..],
        [row(5, "u-1", "u-2")]
    );
    assert_eq!(
        app.get("/api/v2/assets/clusters/templates", Some(&manager))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let response = app
        .get("/api/v2/assets/clusters?search=u-2", Some(&manager))
        .await;
    assert_eq!(response.body["total"], json!(1), "{}", response.body);

    // Purge: what the item holds leaves it to the classic interface.
    app.sql(&format!(
        "INSERT INTO glpi_items_racks (racks_id, itemtype, items_id, position) VALUES ({second}, 'Computer', 102, 1); \
         INSERT INTO glpi_contracts_items (contracts_id, itemtype, items_id) VALUES (1, 'Rack', {rack}); \
         UPDATE glpi_racks SET is_deleted = 1 WHERE id = {second}"
    ))
    .await;
    app.sql("INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'infocom', 31)")
        .await;
    let response = app
        .send(
            Method::DELETE,
            &format!("/api/v2/assets/racks/{second}"),
            Some(&manager),
            None,
        )
        .await;
    assert_eq!(response.error_code(), "legacy_required");
    let response = app
        .send(
            Method::DELETE,
            &format!("/api/v2/assets/racks/{rack}"),
            Some(&manager),
            None,
        )
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        count(
            &app,
            &format!(
                "SELECT (SELECT COUNT(*) FROM glpi_racks WHERE id = {rack}) \
                 + (SELECT COUNT(*) FROM glpi_contracts_items WHERE itemtype = 'Rack') \
                 + (SELECT COUNT(*) FROM glpi_infocoms WHERE itemtype = 'Rack' AND items_id = {rack})"
            )
        )
        .await,
        0
    );
}

#[tokio::test]
async fn software_like_php() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'software', 31), (4, 'infocom', 31); \
         INSERT INTO glpi_softwarecategories (id, name, completename, level) VALUES (5, 'Office', 'Office', 1); \
         INSERT INTO glpi_configs (context, name, value) VALUES \
           ('core', 'default_software_helpdesk_visible', '1'), ('core', 'auto_create_infocoms', '1')",
    )
    .await;
    // Business rules are for the inventory assets only.
    rule(&app, 9, 3, &[], &[("assign", "comment", "by rule")]).await;
    let manager = app.login("manager").await;
    let column = |sql: String| {
        let app = &app;
        async move { column(app, &sql).await }
    };

    // As the classic form creates it: associable to tickets as set, an
    // infocom, no category without category rules, the software events.
    let response = app
        .post(
            "/api/v2/assets/softwares",
            &manager,
            Some(json!({ "entity_id": 1, "name": "Editor" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let editor = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(response.body["kind"], json!("softwares"));
    assert_eq!(response.body["is_helpdesk_visible"], json!(true));
    assert_eq!(response.body["category"], Value::Null);
    assert_eq!(response.body["valid_licenses"], json!(true));
    assert_eq!(response.body["serial"], Value::Null);
    assert_eq!(response.body["state"], Value::Null);
    assert_eq!(
        column(format!(
            "SELECT CAST(CONCAT_WS('|', softwarecategories_id, is_helpdesk_visible, IFNULL(comment, '-')) AS CHAR) \
             FROM glpi_softwares WHERE id = {editor}"
        ))
        .await
        .as_deref(),
        Some("0|1|")
    );
    assert_eq!(
        count(
            &app,
            &format!("SELECT COUNT(*) FROM glpi_infocoms WHERE itemtype = 'Software' AND items_id = {editor}")
        )
        .await,
        1
    );
    assert_eq!(history(&app, "Software", editor).await, [item_row(20)]);
    let response = app
        .get("/api/v2/assets/softwares/defaults", Some(&manager))
        .await;
    assert_eq!(response.body, json!({ "is_helpdesk_visible": true }));
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            editor,
            "software".into(),
            "inventory".into(),
            4,
            "manager adds the item Editor".into()
        )
    );

    // Its fields in the history, as the PHP options; the software it
    // upgrades is not, and goes with the upgrade flag.
    let response = app
        .post(
            "/api/v2/assets/softwares",
            &manager,
            Some(json!({ "entity_id": 1, "name": "Editor 2", "category_id": 5 })),
        )
        .await;
    let editor2 = response.body["id"].as_i64().unwrap() as i32;
    let response = app
        .patch(
            &format!("/api/v2/assets/softwares/{editor2}"),
            &manager,
            json!({ "is_update": true, "upgrade_from_id": editor, "is_helpdesk_visible": false, "category_id": 0 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["upgrade_from"]["name"], json!("Editor"));
    assert_eq!(
        history(&app, "Software", editor2).await[1..],
        [row(62, "Office (5)", "&nbsp; (0)"), row(61, "1", "0")]
    );
    let response = app
        .patch(
            &format!("/api/v2/assets/softwares/{editor2}"),
            &manager,
            json!({ "is_update": false }),
        )
        .await;
    assert_eq!(response.body["upgrade_from"], Value::Null);
    assert_eq!(history(&app, "Software", editor2).await.len(), 3);

    // Category rules choose the category of a new software given none.
    app.sql(
        "INSERT INTO glpi_rules (entities_id, sub_type, ranking, name, `match`, is_active) \
         VALUES (0, 'RuleSoftwareCategory', 1, 'Categories', 'AND', 1)",
    )
    .await;
    let response = app
        .post(
            "/api/v2/assets/softwares",
            &manager,
            Some(json!({ "entity_id": 1, "name": "Viewer" })),
        )
        .await;
    assert_eq!(
        response.error_code(),
        "legacy_required",
        "{}",
        response.body
    );
    assert_eq!(response.body["error"]["details"], json!(["business_rules"]));
    let response = app
        .post(
            "/api/v2/assets/softwares",
            &manager,
            Some(json!({ "entity_id": 1, "name": "Viewer", "category_id": 5 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);

    // Its sharing goes to its versions; it stays shared while a software
    // of another entity upgrades it.
    app.sql(&format!(
        "INSERT INTO glpi_softwareversions (id, entities_id, is_recursive, softwares_id, name) \
         VALUES (7, 1, 0, {editor}, '1.0')"
    ))
    .await;
    let response = app
        .patch(
            &format!("/api/v2/assets/softwares/{editor}"),
            &manager,
            json!({ "is_recursive": true }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(
        column("SELECT CAST(is_recursive AS CHAR) FROM glpi_softwareversions WHERE id = 7".into())
            .await
            .as_deref(),
        Some("1")
    );
    app.sql(&format!(
        "INSERT INTO glpi_softwares (id, entities_id, name, is_update, softwares_id) \
         VALUES (90, 2, 'Lyon editor', 1, {editor})"
    ))
    .await;
    let response = app
        .patch(
            &format!("/api/v2/assets/softwares/{editor}"),
            &manager,
            json!({ "is_recursive": false }),
        )
        .await;
    assert_eq!(response.error_code(), "sharing_in_use", "{}", response.body);

    // Purge: its versions leave it to the classic interface; then the
    // software upgrading it forgets it.
    app.post(
        &format!("/api/v2/assets/softwares/{editor}/trash"),
        &manager,
        None,
    )
    .await;
    let response = app
        .send(
            Method::DELETE,
            &format!("/api/v2/assets/softwares/{editor}"),
            Some(&manager),
            None,
        )
        .await;
    assert_eq!(
        response.error_code(),
        "legacy_required",
        "{}",
        response.body
    );
    app.sql("DELETE FROM glpi_softwareversions WHERE id = 7")
        .await;
    let response = app
        .send(
            Method::DELETE,
            &format!("/api/v2/assets/softwares/{editor}"),
            Some(&manager),
            None,
        )
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        column("SELECT CAST(softwares_id AS CHAR) FROM glpi_softwares WHERE id = 90".into())
            .await
            .as_deref(),
        Some("0")
    );
}

#[tokio::test]
async fn software_versions_like_php() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'software', 31), (1, 'software', 1); \
         INSERT INTO glpi_operatingsystems (id, name) VALUES (3, 'Linux'); \
         UPDATE glpi_states SET is_visible_softwareversion = 0 WHERE id = 1; \
         INSERT INTO glpi_softwares (id, entities_id, is_recursive, name) VALUES (40, 1, 1, 'Editor'), (41, 2, 0, 'Lyon')",
    )
    .await;
    let manager = app.login("manager").await;
    let versions = "/api/v2/assets/softwares/40/versions";

    // Added in the entity of the software, with its sharing; the software
    // notes it, the version its creation, and the event is the software's.
    let response = app
        .post(
            versions,
            &manager,
            Some(json!({ "name": "1.0", "comment": "first", "operating_system_id": 3, "state_id": 2 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let version = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(response.body["entity"]["id"], json!(1));
    assert_eq!(response.body["is_recursive"], json!(true));
    assert_eq!(response.body["operating_system"]["name"], json!("Linux"));
    assert_eq!(response.body["state"]["name"], json!("Stock"));
    let name = format!("Editor - 1.0 ({version})");
    let subitem = |action: i32, old: &str, new: &str| {
        (
            0,
            old.to_owned(),
            new.to_owned(),
            "SoftwareVersion".to_owned(),
            action,
            "manager (19)".to_owned(),
        )
    };
    assert_eq!(
        history(&app, "Software", 40).await,
        [subitem(17, "", &name)]
    );
    assert_eq!(
        history(&app, "SoftwareVersion", version).await,
        [item_row(20)]
    );
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            40,
            "software".into(),
            "inventory".into(),
            4,
            format!("manager adds the version {version}")
        )
    );

    // Changed: its own history for the fields having a search option,
    // the raw values in the history of the software.
    let response = app
        .patch(
            &format!("{versions}/{version}"),
            &manager,
            json!({ "name": "1.1", "operating_system_id": null, "state_id": 2 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["operating_system"], Value::Null);
    assert_eq!(
        history(&app, "SoftwareVersion", version).await[1..],
        [row(2, "1.0", "1.1"), row(4, "Linux (3)", "&nbsp; (0)")]
    );
    assert_eq!(
        history(&app, "Software", 40).await[1..],
        [subitem(18, "1.0", "1.1"), subitem(18, "3", "0")]
    );
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        format!("manager updates the version {version}")
    );
    let response = app.get(versions, Some(&manager)).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["can_edit"], json!(true));
    assert_eq!(response.body["items"][0]["name"], json!("1.1"));

    // The statuses offered for the versions; their fields are the
    // software's only.
    let response = app
        .get(
            "/api/v2/assets/softwares/choices/version_state?entity_id=1",
            Some(&manager),
        )
        .await;
    assert_eq!(
        response.body,
        json!({ "items": [{ "id": 2, "label": "Stock" }] })
    );
    let response = app
        .get(
            "/api/v2/assets/softwares/choices/version_operating_system?entity_id=1",
            Some(&manager),
        )
        .await;
    assert_eq!(
        response.body,
        json!({ "items": [{ "id": 3, "label": "Linux" }] })
    );
    let response = app
        .get(
            "/api/v2/assets/computers/choices/version_state?entity_id=1",
            Some(&manager),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // Invalid values; unknown references.
    let response = app
        .post(versions, &manager, Some(json!({ "name": "x".repeat(256) })))
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let response = app
        .post(
            versions,
            &manager,
            Some(json!({ "operating_system_id": 99 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // The rights of the software: read only for the technician; another
    // entity is not visible.
    let paris = app.login("paris").await;
    let response = app.get(versions, Some(&paris)).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["can_edit"], json!(false));
    let response = app.post(versions, &paris, Some(json!({}))).await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let response = app
        .get("/api/v2/assets/softwares/41/versions", Some(&paris))
        .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    let response = app
        .patch(
            &format!("/api/v2/assets/softwares/41/versions/{version}"),
            &manager,
            json!({}),
        )
        .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);

    // Purge: licenses bought or used with it leave it to the classic
    // interface; then its installations and its history go with it.
    app.sql(&format!(
        "INSERT INTO glpi_softwarelicenses (id, softwares_id, entities_id, name, softwareversions_id_buy) \
         VALUES (5, 40, 1, 'L', {version}); \
         INSERT INTO glpi_items_softwareversions (items_id, itemtype, softwareversions_id, entities_id) \
         VALUES (105, 'Computer', {version}, 1)"
    ))
    .await;
    let uri = format!("{versions}/{version}");
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(
        response.error_code(),
        "legacy_required",
        "{}",
        response.body
    );
    app.sql("DELETE FROM glpi_softwarelicenses").await;
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        count(
            &app,
            &format!(
                "SELECT COUNT(*) FROM glpi_items_softwareversions WHERE softwareversions_id = {version}"
            )
        )
        .await,
        0
    );
    assert!(history(&app, "SoftwareVersion", version).await.is_empty());
    assert_eq!(
        history(&app, "Software", 40).await[3..],
        [subitem(19, &format!("Editor - 1.1 ({version})"), "")]
    );
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        format!("manager purges the version {version}")
    );
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn software_installations_like_php() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'software', 31), (1, 'software', 1); \
         INSERT INTO glpi_softwares (id, entities_id, is_recursive, name) VALUES (40, 1, 1, 'Editor'), (41, 2, 0, 'Lyon'); \
         INSERT INTO glpi_softwareversions (id, softwares_id, entities_id, is_recursive, name) VALUES \
           (50, 40, 1, 1, '1.0'), (51, 41, 2, 0, '2.0'); \
         INSERT INTO glpi_phones (id, name, entities_id) VALUES (60, 'PHONE-A', 3)",
    )
    .await;
    let manager = app.login("manager").await;
    let link = |action: i32, old: &str, new: &str, other: &str| {
        (
            0,
            old.to_owned(),
            new.to_owned(),
            other.to_owned(),
            action,
            "manager (19)".to_owned(),
        )
    };

    // Installed like the form of the software tab: in the entity of the
    // computer, noted on both sides, with the event of the form.
    let response = app
        .post(
            "/api/v2/assets/computers/102/software",
            &manager,
            Some(json!({ "version_id": 50 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let installation = response.body["id"].as_i64().unwrap();
    assert_eq!(response.body["software"]["name"], json!("Editor"));
    assert_eq!(response.body["version"]["name"], json!("1.0"));
    assert_eq!(
        column(
            &app,
            &format!(
                "SELECT CAST(CONCAT_WS('|', entities_id, is_deleted_item, is_template_item, is_dynamic) AS CHAR) \
                 FROM glpi_items_softwareversions WHERE id = {installation}"
            )
        )
        .await
        .as_deref(),
        Some("1|0|0|0")
    );
    assert_eq!(
        history(&app, "Computer", 102).await,
        [link(4, "", "Editor - 1.0 (50)", "SoftwareVersion")]
    );
    assert_eq!(
        history(&app, "SoftwareVersion", 50).await,
        [link(4, "", "PC-PARIS-1 (102)", "Computer")]
    );
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            102,
            "Computer".into(),
            "inventory".into(),
            5,
            "manager installs software".into()
        )
    );
    let response = app
        .get("/api/v2/assets/computers/102/software", Some(&manager))
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["can_edit"], json!(true));
    assert_eq!(response.body["items"].as_array().unwrap().len(), 1);

    // Twice: refused. A version of another entity: refused.
    let response = app
        .post(
            "/api/v2/assets/computers/102/software",
            &manager,
            Some(json!({ "version_id": 50 })),
        )
        .await;
    assert_eq!(
        response.error_code(),
        "already_installed",
        "{}",
        response.body
    );
    let response = app
        .post(
            "/api/v2/assets/computers/102/software",
            &manager,
            Some(json!({ "version_id": 51 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let response = app
        .post(
            "/api/v2/assets/computers/102/software",
            &manager,
            Some(json!({ "version_id": 99 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let response = app
        .get("/api/v2/assets/racks/1/software", Some(&manager))
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // The software to install: those of the entity of the asset or shared
    // with it, outside the trash and the templates.
    app.sql(
        "INSERT INTO glpi_softwares (id, entities_id, name, is_deleted) VALUES (42, 1, 'Old', 1)",
    )
    .await;
    let response = app
        .get(
            "/api/v2/assets/computers/choices/software?entity_id=1",
            Some(&manager),
        )
        .await;
    assert_eq!(
        response.body,
        json!({ "items": [{ "id": 40, "label": "Editor" }] })
    );

    // A phone does not give its entity: the active one of the session.
    let response = app
        .post(
            "/api/v2/assets/phones/60/software",
            &manager,
            Some(json!({ "version_id": 50 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    assert_eq!(
        column(
            &app,
            "SELECT CAST(entities_id AS CHAR) FROM glpi_items_softwareversions WHERE itemtype = 'Phone'"
        )
        .await
        .as_deref(),
        Some("1")
    );

    // The update right on the computer is enough with the view of the
    // software; the view of both is not.
    let paris = app.login("paris").await;
    let response = app
        .post(
            "/api/v2/assets/computers/103/software",
            &paris,
            Some(json!({ "version_id": 50 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let response = app
        .get("/api/v2/assets/computers/103/software", Some(&paris))
        .await;
    assert_eq!(response.body["can_edit"], json!(false));
    app.sql("UPDATE glpi_profilerights SET rights = 1 WHERE profiles_id = 1 AND name = 'computer'")
        .await;
    let paris = app.login("paris").await;
    let response = app
        .post(
            "/api/v2/assets/computers/102/software",
            &paris,
            Some(json!({ "version_id": 50 })),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    // Uninstalled for good, noted on both sides, without event.
    let before = events(&app).await.len();
    let uri = format!("/api/v2/assets/computers/102/software/{installation}");
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        history(&app, "Computer", 102).await[1..],
        [link(5, "Editor - 1.0 (50)", "", "SoftwareVersion")]
    );
    assert_eq!(
        history(&app, "SoftwareVersion", 50).await.last().unwrap(),
        &link(5, "PC-PARIS-1 (102)", "", "Computer")
    );
    assert_eq!(events(&app).await.len(), before);
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);

    // Without the view of the software, no software tab.
    let helpdesk = app.login("helpdesk").await;
    let response = app
        .get("/api/v2/assets/computers/102/software", Some(&helpdesk))
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn software_licenses_like_php() {
    let app = test_app!();
    app.sql(
        "INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES (4, 'software', 31), (4, 'license', 31); \
         INSERT INTO glpi_softwares (id, entities_id, name) VALUES (40, 1, 'Editor'); \
         INSERT INTO glpi_softwareversions (id, softwares_id, entities_id, name) VALUES (50, 40, 1, '1.0'), (51, 40, 1, '2.0'); \
         INSERT INTO glpi_softwarelicenses (id, softwares_id, entities_id, name, completename, level, number) \
         VALUES (60, 40, 1, 'Site', 'Site', 1, 5)",
    )
    .await;
    let manager = app.login("manager").await;
    let licenses = "/api/v2/assets/software-licenses";

    // Created under a parent: its full name and level, the event of its
    // software. The name loses its spaces; the software is required.
    let response = app
        .post(
            licenses,
            &manager,
            Some(json!({ "entity_id": 1, "name": "Seats" })),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let response = app
        .post(
            licenses,
            &manager,
            Some(json!({
                "entity_id": 1, "software_id": 40, "name": " Seats ", "parent_license_id": 60,
                "number": 1, "expire": "2027-06-30", "version_use_id": 51, "allow_overquota": true
            })),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    let license = response.body["id"].as_i64().unwrap() as i32;
    assert_eq!(response.body["name"], json!("Seats"));
    assert_eq!(response.body["full_name"], json!("Site > Seats"));
    assert_eq!(response.body["software"]["name"], json!("Editor"));
    assert_eq!(response.body["version_use"]["name"], json!("2.0"));
    assert_eq!(response.body["expire"], json!("2027-06-30"));
    assert_eq!(response.body["is_valid"], json!(true));
    assert_eq!(
        column(
            &app,
            &format!("SELECT CAST(level AS CHAR) FROM glpi_softwarelicenses WHERE id = {license}")
        )
        .await
        .as_deref(),
        Some("2")
    );
    assert_eq!(
        history(&app, "SoftwareLicense", license).await,
        [item_row(20)]
    );
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            40,
            "software".into(),
            "inventory".into(),
            4,
            format!("manager adds the license {license}")
        )
    );

    // Each update computes its validity from its affectations: invalid,
    // the software too. A new parent: a new full name, after the fields.
    app.sql(&format!(
        "INSERT INTO glpi_items_softwarelicenses (items_id, itemtype, softwarelicenses_id) \
         VALUES (102, 'Computer', {license}), (103, 'Computer', {license})"
    ))
    .await;
    let response = app
        .patch(
            &format!("{licenses}/{license}"),
            &manager,
            json!({ "name": "Seats 2", "parent_license_id": null, "expire": null }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["is_valid"], json!(false));
    assert_eq!(response.body["full_name"], json!("Seats 2"));
    assert_eq!(
        history(&app, "SoftwareLicense", license).await[1..],
        [
            row(1, "Seats", "Seats 2"),
            row(13, "60", "0"),
            row(8, "2027-06-30", "NULL"),
            row(13, "Site &gt; Seats", "Seats 2"),
            row(9, "1", "0"),
        ]
    );
    assert_eq!(history(&app, "Software", 40).await, [row(63, "1", "0")]);
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        format!("manager updates the license {license}")
    );
    // Not under itself; its software is chosen at its creation.
    let response = app
        .patch(
            &format!("{licenses}/60"),
            &manager,
            json!({ "parent_license_id": 60 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let response = app
        .patch(
            &format!("{licenses}/{license}"),
            &manager,
            json!({ "software_id": 40 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // The parents of the form: the licenses of the software, not itself.
    let response = app
        .get(
            &format!(
                "/api/v2/assets/software-licenses/choices/parent_license?entity_id=1&software_id=40&exclude_id={license}"
            ),
            Some(&manager),
        )
        .await;
    assert_eq!(
        response.body,
        json!({ "items": [{ "id": 60, "label": "Site" }] })
    );
    let response = app
        .get(
            "/api/v2/assets/software-licenses/choices/license_version?entity_id=1&software_id=40",
            Some(&manager),
        )
        .await;
    assert_eq!(response.body["items"].as_array().unwrap().len(), 2);

    // Trash: not with licenses under it; its restoration is logged on it.
    app.sql(&format!(
        "UPDATE glpi_softwarelicenses SET softwarelicenses_id = 60 WHERE id = {license}"
    ))
    .await;
    let response = app
        .post(&format!("{licenses}/60/trash"), &manager, None)
        .await;
    assert_eq!(
        response.error_code(),
        "legacy_required",
        "{}",
        response.body
    );
    let response = app
        .post(&format!("{licenses}/{license}/trash"), &manager, None)
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        format!("manager deletes the license {license}")
    );
    app.post(&format!("{licenses}/{license}/restore"), &manager, None)
        .await;
    assert_eq!(
        events(&app).await.pop().unwrap(),
        (
            license,
            "software".into(),
            "inventory".into(),
            4,
            "manager restores an item".into()
        )
    );

    // Purge: not with affectations; then the software is valid again.
    app.post(&format!("{licenses}/{license}/trash"), &manager, None)
        .await;
    let uri = format!("{licenses}/{license}");
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(
        response.error_code(),
        "legacy_required",
        "{}",
        response.body
    );
    app.sql("DELETE FROM glpi_items_softwarelicenses").await;
    let response = app.send(Method::DELETE, &uri, Some(&manager), None).await;
    assert_eq!(response.status, StatusCode::NO_CONTENT, "{}", response.body);
    assert_eq!(
        history(&app, "Software", 40).await[1..],
        [row(63, "0", "1")]
    );
    assert_eq!(
        events(&app).await.pop().unwrap().4,
        format!("manager purges the license {license}")
    );

    // The licenses of a software.
    let response = app
        .get(
            "/api/v2/assets/software-licenses?software_id=40",
            Some(&manager),
        )
        .await;
    assert_eq!(response.body["total"], json!(1));
    let response = app
        .get(
            "/api/v2/assets/software-licenses?software_id=41",
            Some(&manager),
        )
        .await;
    assert_eq!(response.body["total"], json!(0));

    // Their templates stay in the classic interface.
    let response = app
        .get("/api/v2/assets/software-licenses/templates", Some(&manager))
        .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}
