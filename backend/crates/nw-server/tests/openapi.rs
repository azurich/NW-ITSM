//! `backend/openapi.json` is the API contract used to generate the
//! frontend types. It must match the code: regenerate it with
//! `UPDATE_OPENAPI=1 cargo test -p nw-server --test openapi`.

use std::path::Path;

#[test]
fn committed_openapi_document_is_up_to_date() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../openapi.json");
    let generated = nw_server::openapi::json();
    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == generated,
        "backend/openapi.json is outdated: run `UPDATE_OPENAPI=1 cargo test -p nw-server --test openapi`"
    );
}

/// utoipa does not register the schemas of tuple path parameters: every
/// reference must point to a declared schema, or the type generation fails.
#[test]
fn every_schema_reference_is_declared() {
    let document: serde_json::Value = serde_json::from_str(&nw_server::openapi::json()).unwrap();
    let schemas = &document["components"]["schemas"];
    let mut missing = Vec::new();
    collect_missing(&document, schemas, &mut missing);
    assert!(missing.is_empty(), "undeclared schemas: {missing:?}");
}

fn collect_missing(
    value: &serde_json::Value,
    schemas: &serde_json::Value,
    missing: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(|reference| reference.as_str()) {
                let name = reference.trim_start_matches("#/components/schemas/");
                if schemas.get(name).is_none() {
                    missing.push(name.to_owned());
                }
            }
            for child in map.values() {
                collect_missing(child, schemas, missing);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_missing(item, schemas, missing);
            }
        }
        _ => {}
    }
}
