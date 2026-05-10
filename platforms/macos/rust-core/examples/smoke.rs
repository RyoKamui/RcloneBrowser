use std::{
    ffi::{CStr, CString},
    fs, thread,
    time::Duration,
};

use serde_json::{Value, json};

fn call(command: &str, payload: Value) -> Value {
    let request =
        CString::new(json!({ "command": command, "payload": payload }).to_string()).unwrap();
    let pointer = unsafe { rclone_browser_core::rb_call(request.as_ptr()) };
    assert!(!pointer.is_null(), "{command} returned a null pointer");
    let text = unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned();
    unsafe { rclone_browser_core::rb_string_free(pointer) };
    let response: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(response["ok"], true, "{command} failed: {text}");
    response["data"].clone()
}

fn main() {
    let bootstrap = call("bootstrap", json!({}));
    assert!(
        bootstrap["rclone"]["available"].as_bool().unwrap_or(false),
        "rclone is unavailable"
    );
    assert!(
        bootstrap["remotes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|remote| remote["name"] == "__local__")
    );

    let providers = call("listProviders", json!({}));
    assert!(
        providers.as_array().is_some_and(|items| items.len() > 20),
        "provider registry is unexpectedly small"
    );

    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let source = root.join("source.txt");
    let copied = root.join("copied.txt");
    fs::write(&source, b"native rust bridge smoke test\n").unwrap();

    let listing = call(
        "listEntries",
        json!({ "remote": "__local__", "path": root, "sharedWithMe": false }),
    );
    assert!(
        listing
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "source.txt")
    );

    let folder = root.join("folder");
    call(
        "createFolder",
        json!({ "remote": "__local__", "path": folder }),
    );
    assert!(folder.is_dir());
    call(
        "renameEntry",
        json!({ "remote": "__local__", "path": folder, "newName": "renamed" }),
    );
    let renamed = root.join("renamed");
    assert!(renamed.is_dir());
    let moved = root.join("moved");
    call(
        "moveEntry",
        json!({
            "remote": "__local__", "source": renamed,
            "destination": moved, "sharedWithMe": false
        }),
    );
    assert!(moved.is_dir());
    assert!(!renamed.exists());

    let transfer = call(
        "startTransfer",
        json!({
            "direction": "copy", "operation": "copy", "source": source,
            "destination": copied, "isDirectory": false, "extraArgs": [], "label": "Smoke test"
        }),
    );
    let id = transfer["id"].as_str().unwrap();
    let mut completed = false;
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(50));
        let transfers = call("listTransfers", json!({}));
        let current = transfers
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap();
        match current["status"].as_str() {
            Some("completed") => {
                completed = true;
                break;
            }
            Some("failed") => panic!("transfer failed: {current}"),
            _ => {}
        }
    }
    assert!(completed, "transfer did not complete in time");
    assert_eq!(fs::read(&copied).unwrap(), fs::read(&source).unwrap());

    let summary = call(
        "directorySize",
        json!({ "remote": "__local__", "path": root, "sharedWithMe": false }),
    );
    assert!(summary["count"].as_u64().unwrap_or(0) >= 2);
    let tree = call(
        "directoryTree",
        json!({ "remote": "__local__", "path": root, "sharedWithMe": false }),
    );
    assert!(tree.as_str().is_some_and(|value| value.contains("moved")));
    let export = root.join("listing.csv");
    let count = call(
        "exportListing",
        json!({
            "remote": "__local__", "path": root, "destination": export,
            "format": "csv", "sharedWithMe": false
        }),
    );
    assert!(count.as_u64().unwrap_or(0) >= 2);
    assert!(
        fs::read_to_string(&export)
            .unwrap()
            .starts_with("Path,Modified,Size")
    );
    call(
        "deleteEntry",
        json!({
            "remote": "__local__", "path": moved, "isDir": true,
            "sharedWithMe": false
        }),
    );
    assert!(!moved.exists());
    call("clearFinishedWork", json!({}));

    println!(
        "native bridge smoke test passed: providers, browsing, CRUD/move, transfer, progress, size, tree, export, and cleanup"
    );
}
