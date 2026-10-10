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

    // Business rule on manual updates of computers.
    app.sql(
        "INSERT INTO glpi_rules (id, sub_type, name, `match`, is_active, `condition`) VALUES \
         (1, 'RuleAsset', 'Set the location', 'AND', 1, 2); \
         INSERT INTO glpi_rulecriterias (rules_id, criteria, `condition`, pattern) VALUES \
         (1, 'name', 0, 'x'), (1, '_itemtype', 0, 'Computer')",
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
    // Not on creations, nor on the other types.
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(create.clone()))
        .await;
    assert_eq!(response.status, StatusCode::CREATED);
    let response = app
        .patch(
            "/api/v2/assets/monitors/200",
            &manager,
            json!({ "name": "x" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    // A rule limited to inventory tools does not apply.
    app.sql("INSERT INTO glpi_rulecriterias (rules_id, criteria, `condition`, pattern) VALUES (1, '_auto', 0, '1')")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "name": "x" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);

    // Location change propagated to a monitor, with a rule on monitors.
    app.sql(
        "INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'is_location_autoupdate', '1'); \
         INSERT INTO glpi_rules (id, sub_type, name, `match`, is_active, `condition`) VALUES \
         (2, 'RuleAsset', 'Monitors', 'AND', 1, 2); \
         INSERT INTO glpi_rulecriterias (rules_id, criteria, `condition`, pattern) VALUES (2, '_itemtype', 0, 'Monitor')",
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "location_id": 3 }),
        )
        .await;
    refused(&response, "business_rules");
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "serial": "y" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    app.sql("DELETE FROM glpi_rules").await;

    // Unicity of fields.
    app.sql(
        "INSERT INTO glpi_fieldunicities (name, itemtype, entities_id, fields, is_active, action_refuse) \
         VALUES ('Serial', 'Computer', 0, 'serial', 1, 1)",
    )
    .await;
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "serial": "z" }),
        )
        .await;
    refused(&response, "field_unicity");
    app.sql("DELETE FROM glpi_fieldunicities").await;

    // Financial information created with the items.
    app.sql("INSERT INTO glpi_configs (context, name, value) VALUES ('core', 'auto_create_infocoms', '1')")
        .await;
    let response = app
        .post("/api/v2/assets/computers", &manager, Some(create.clone()))
        .await;
    refused(&response, "infocom_creation");
    app.sql("DELETE FROM glpi_configs WHERE name = 'auto_create_infocoms'")
        .await;

    // Status filling a date of the financial information, set on the root
    // entity and inherited by Paris.
    app.sql("UPDATE glpi_entities SET autofill_use_date = 'STATUS_1' WHERE id = 0")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/monitors/200",
            &manager,
            json!({ "state_id": 1 }),
        )
        .await;
    refused(&response, "infocom_dates");
    let mut with_state = create.clone();
    with_state["state_id"] = json!(1);
    let response = app
        .post("/api/v2/assets/monitors", &manager, Some(with_state))
        .await;
    refused(&response, "infocom_dates");
    let response = app
        .patch(
            "/api/v2/assets/monitors/200",
            &manager,
            json!({ "state_id": 2 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    app.sql("UPDATE glpi_entities SET autofill_use_date = '0' WHERE id = 1")
        .await;
    let response = app
        .patch(
            "/api/v2/assets/monitors/200",
            &manager,
            json!({ "state_id": 1 }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);

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

    // Shared items cannot be unshared here.
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": true }),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["is_recursive"], true);
    let response = app
        .patch(
            "/api/v2/assets/computers/102",
            &manager,
            json!({ "is_recursive": false }),
        )
        .await;
    refused(&response, "unrecursive");

    // Refusals write nothing.
    let names: Vec<_> = history(&app, "Computer", 102)
        .await
        .into_iter()
        .map(|entry| entry.0)
        .collect();
    assert_eq!(names, [1, 5, 86]);
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
