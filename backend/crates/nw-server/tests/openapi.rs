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
