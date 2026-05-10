#![cfg(unix)]

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

fn wait_for(command: &str, id: &str) -> Value {
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(20));
        let items = call(command, json!({}));
        let item = items
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .cloned()
            .unwrap();
        match item["status"].as_str() {
            Some("completed") => return item,
            Some("failed" | "cancelled") => panic!("work did not complete: {item}"),
            _ => {}
        }
    }
    panic!("work did not finish in time");
}

#[test]
fn exercises_the_native_bridge_against_a_cli_fixture() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../desktop/tests/fixtures/fake-rclone")
        .canonicalize()
        .unwrap();
    let data = tempfile::tempdir().unwrap();
    let export = data.path().join("listing.csv");
    let settings = json!({
        "rclonePath": fixture,
        "showHidden": true,
        "streamCommand": "/usr/bin/wc -c"
    });
    fs::write(
        data.path().join("settings.json"),
        serde_json::to_vec_pretty(&settings).unwrap(),
    )
    .unwrap();

    // The core is process-global. This integration test is its own test binary,
    // so the override is installed before the first bridge call.
    unsafe { std::env::set_var("RCLONE_BROWSER_DATA_DIR", data.path()) };

    let bootstrap = call("bootstrap", json!({}));
    assert_eq!(bootstrap["rclone"]["available"], true);
    assert_eq!(bootstrap["rclone"]["version"], "rclone v1.99.0-fixture");
    assert!(
        bootstrap["remotes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|remote| { remote["name"] == "fixture" && remote["type"] == "drive" })
    );

    let providers = call("listProviders", json!({}));
    assert_eq!(providers.as_array().unwrap().len(), 1);
    assert_eq!(providers[0]["name"], "fixture");

    let question = call(
        "startConfig",
        json!({ "name": "new-fixture", "provider": "fixture" }),
    );
    assert_eq!(question["option"]["name"], "endpoint");
    let completed = call(
        "continueConfig",
        json!({
            "name": "new-fixture", "provider": "fixture",
            "state": question["state"], "result": "https://example.invalid"
        }),
    );
    assert_eq!(completed["state"], "");
    call("deleteRemote", json!({ "name": "new-fixture" }));

    let update = call("startUpdate", json!({ "name": "fixture" }));
    let update_completed = call(
        "continueUpdate",
        json!({
            "name": "fixture", "state": update["state"],
            "result": "https://example.invalid"
        }),
    );
    assert_eq!(update_completed["state"], "");

    let entries = call(
        "listEntries",
        json!({ "remote": "fixture", "path": "", "sharedWithMe": true }),
    );
    assert_eq!(entries[0]["name"], "Docs");
    assert_eq!(entries[0]["size"], Value::Null);
    assert_eq!(entries[1]["size"], 12);

    call(
        "createFolder",
        json!({ "remote": "fixture", "path": "Created" }),
    );
    call(
        "renameEntry",
        json!({ "remote": "fixture", "path": "Created", "newName": "Renamed" }),
    );
    call(
        "moveEntry",
        json!({
            "remote": "fixture", "source": "Renamed",
            "destination": "Docs/Renamed"
        }),
    );
    call(
        "deleteEntry",
        json!({
            "remote": "fixture", "path": "Docs/Renamed", "isDir": true
        }),
    );

    assert_eq!(
        call(
            "publicLink",
            json!({ "remote": "fixture", "path": "readme.txt" }),
        ),
        "https://example.invalid/public-link"
    );
    let size = call("directorySize", json!({ "remote": "fixture", "path": "" }));
    assert_eq!(size, json!({ "count": 3, "bytes": 75 }));
    assert!(
        call("directoryTree", json!({ "remote": "fixture", "path": "" }))
            .as_str()
            .unwrap()
            .contains("Archive")
    );
    assert_eq!(
        call(
            "exportListing",
            json!({
                "remote": "fixture", "path": "", "destination": export,
                "format": "csv", "sharedWithMe": true
            }),
        ),
        2
    );
    assert!(
        fs::read_to_string(&export)
            .unwrap()
            .starts_with("Path,Modified,Size\n")
    );

    let transfer = call(
        "startTransfer",
        json!({
            "direction": "download", "operation": "copy",
            "source": "fixture:readme.txt", "destination": data.path().join("readme.txt"),
            "isDirectory": false, "extraArgs": [], "label": "Fixture transfer"
        }),
    );
    let finished = wait_for("listTransfers", transfer["id"].as_str().unwrap());
    assert_eq!(finished["bytes"], 75);
    assert_eq!(finished["totalBytes"], 75);
    let copied_command = call(
        "copyCommand",
        json!({
            "direction": "download", "operation": "copy",
            "source": "fixture:readme.txt", "destination": data.path().join("a file.txt"),
            "isDirectory": false, "extraArgs": ["--checksum"]
        }),
    );
    assert!(
        copied_command
            .as_str()
            .unwrap()
            .contains("copyto --checksum")
    );
    assert!(copied_command.as_str().unwrap().contains('\''));

    let slow_transfer = call(
        "startTransfer",
        json!({
            "direction": "download", "operation": "copy",
            "source": "fixture:slow", "destination": data.path().join("slow"),
            "isDirectory": false, "extraArgs": [], "label": "Cancelled transfer"
        }),
    );
    thread::sleep(Duration::from_millis(50));
    call("cancelTransfer", json!({ "id": slow_transfer["id"] }));
    let cancelled = call("listTransfers", json!({}));
    assert!(
        cancelled
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["id"] == slow_transfer["id"] && item["status"] == "cancelled" })
    );

    let saved = call(
        "saveTask",
        json!({
            "description": "Fixture task", "source": "fixture:Docs",
            "destination": data.path().join("Docs"), "isDirectory": true
        }),
    );
    assert_eq!(call("listTasks", json!({})).as_array().unwrap().len(), 1);
    let task_command = call("taskCommand", json!({ "task": saved, "dryRun": true }));
    assert!(task_command.as_str().unwrap().contains("--dry-run"));
    let task_transfer = call("runTask", json!({ "id": saved["id"], "dryRun": true }));
    wait_for("listTransfers", task_transfer["id"].as_str().unwrap());
    call("deleteTask", json!({ "id": saved["id"] }));

    let stream = call(
        "startStream",
        json!({ "source": "fixture:readme.txt", "command": "" }),
    );
    wait_for("listActivities", stream["id"].as_str().unwrap());

    let mount = call(
        "startMount",
        json!({
            "source": "fixture:", "destination": data.path().join("mount"),
            "extraArgs": []
        }),
    );
    wait_for("listActivities", mount["id"].as_str().unwrap());

    assert_eq!(call("configFile", json!({})), "/tmp/rclone-fixture.conf");
    let update = call("checkRcloneUpdate", json!({}));
    assert_eq!(update["currentVersion"], "1.99.0-fixture");
    call("clearFinishedWork", json!({}));
    assert!(
        call("listTransfers", json!({}))
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        call("listActivities", json!({}))
            .as_array()
            .unwrap()
            .is_empty()
    );
}
