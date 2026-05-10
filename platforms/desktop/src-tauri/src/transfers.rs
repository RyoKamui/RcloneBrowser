use std::{
    collections::HashMap,
    process::Stdio,
    sync::{Arc, Mutex},
};

use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::oneshot,
};
use uuid::Uuid;

use crate::{
    models::{
        Settings, TransferOperation, TransferRequest, TransferSnapshot, TransferStatus,
        unix_timestamp,
    },
    rclone::RcloneClient,
};

#[derive(Clone, Default)]
pub struct TransferManager {
    snapshots: Arc<Mutex<HashMap<String, TransferSnapshot>>>,
    cancellation: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

impl TransferManager {
    pub async fn start(
        &self,
        app: AppHandle,
        client: RcloneClient,
        settings: Settings,
        password: Option<String>,
        request: TransferRequest,
    ) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        let snapshot = TransferSnapshot::new(id.clone(), &request);
        self.snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.clone(), snapshot.clone());
        emit(&app, &snapshot);

        let operation = match (request.operation, request.is_directory) {
            (TransferOperation::Copy, true) => "copy",
            (TransferOperation::Copy, false) => "copyto",
            (TransferOperation::Move, true) => "move",
            (TransferOperation::Move, false) => "moveto",
            (TransferOperation::Sync, _) => "sync",
        };
        let mut arguments = vec![
            operation.to_owned(),
            request.source.clone(),
            request.destination.clone(),
            "--use-json-log".into(),
            "--stats".into(),
            "1s".into(),
            "--stats-log-level".into(),
            "NOTICE".into(),
            "--log-level".into(),
            "NOTICE".into(),
        ];
        if request.is_directory {
            arguments.push("--create-empty-src-dirs".into());
        }
        arguments.extend(request.extra_args);

        let mut command = client.command(&settings, password.as_deref(), arguments);
        let mut child = match command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                self.finish(
                    &app,
                    &id,
                    TransferStatus::Failed,
                    Some(format!("Could not start rclone: {error}")),
                );
                return Err(format!("Could not start rclone: {error}"));
            }
        };

        self.set_running(&app, &id);
        let process_id = child.id();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancellation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.clone(), cancel_tx);

        tauri::async_runtime::spawn(async move {
            if cancel_rx.await.is_ok()
                && let Some(process_id) = process_id
            {
                terminate_process(process_id).await;
            }
        });

        let manager = self.clone();
        let transfer_id = id.clone();
        let event_app = app.clone();
        let stderr = child.stderr.take();
        let reader = tauri::async_runtime::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.update_from_log(&event_app, &transfer_id, &line);
                }
            }
        });

        let manager = self.clone();
        let transfer_id = id.clone();
        tauri::async_runtime::spawn(async move {
            let result = child.wait().await;
            let _ = reader.await;
            manager
                .cancellation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&transfer_id);

            match result {
                Ok(status) if status.success() => {
                    manager.finish(&app, &transfer_id, TransferStatus::Completed, None)
                }
                Ok(status) => manager.finish(
                    &app,
                    &transfer_id,
                    TransferStatus::Failed,
                    Some(format!("rclone exited with {status}")),
                ),
                Err(error) => manager.finish(
                    &app,
                    &transfer_id,
                    TransferStatus::Failed,
                    Some(format!("Could not wait for rclone: {error}")),
                ),
            }
        });

        Ok(id)
    }

    pub fn list(&self) -> Vec<TransferSnapshot> {
        let mut transfers: Vec<_> = self
            .snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .cloned()
            .collect();
        transfers.sort_by_key(|transfer| std::cmp::Reverse(transfer.started_at));
        transfers
    }

    pub fn cancel(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let sender = self
            .cancellation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(id)
            .ok_or_else(|| "This transfer is no longer running.".to_owned())?;
        let _ = sender.send(());
        self.finish(app, id, TransferStatus::Cancelled, None);
        Ok(())
    }

    pub fn clear_finished(&self) {
        self.snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, transfer| {
                matches!(
                    transfer.status,
                    TransferStatus::Queued | TransferStatus::Running
                )
            });
    }

    pub fn has_running(&self) -> bool {
        !self
            .cancellation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }

    pub fn cancel_all(&self, app: &AppHandle) {
        let ids: Vec<String> = self
            .cancellation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.cancel(app, &id);
        }
    }

    fn set_running(&self, app: &AppHandle, id: &str) {
        let snapshot = {
            let mut snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(snapshot) = snapshots.get_mut(id) else {
                return;
            };
            snapshot.status = TransferStatus::Running;
            snapshot.clone()
        };
        emit(app, &snapshot);
    }

    fn update_from_log(&self, app: &AppHandle, id: &str, line: &str) {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => Value::Null,
        };
        let snapshot = {
            let mut snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(snapshot) = snapshots.get_mut(id) else {
                return;
            };

            if let Some(stats) = value.get("stats") {
                snapshot.bytes = stats
                    .get("bytes")
                    .and_then(Value::as_u64)
                    .unwrap_or(snapshot.bytes);
                snapshot.total_bytes = stats
                    .get("totalBytes")
                    .and_then(Value::as_u64)
                    .or(snapshot.total_bytes);
                snapshot.speed = stats
                    .get("speed")
                    .and_then(Value::as_f64)
                    .or(snapshot.speed);
                snapshot.eta_seconds = stats
                    .get("eta")
                    .and_then(Value::as_f64)
                    .filter(|eta| *eta >= 0.0);
                snapshot.checks = stats
                    .get("checks")
                    .and_then(Value::as_u64)
                    .unwrap_or(snapshot.checks);
                snapshot.total_checks = stats
                    .get("totalChecks")
                    .and_then(Value::as_u64)
                    .or(snapshot.total_checks);
                snapshot.files_transferred = stats
                    .get("transfers")
                    .and_then(Value::as_u64)
                    .unwrap_or(snapshot.files_transferred);
                snapshot.total_files = stats
                    .get("totalTransfers")
                    .and_then(Value::as_u64)
                    .or(snapshot.total_files);
                snapshot.errors = stats
                    .get("errors")
                    .and_then(Value::as_u64)
                    .unwrap_or(snapshot.errors);
                snapshot.elapsed_seconds = stats
                    .get("elapsedTime")
                    .and_then(Value::as_f64)
                    .or(snapshot.elapsed_seconds);
            }

            if let Some(message) = value
                .get("msg")
                .and_then(Value::as_str)
                .or_else(|| value.is_null().then_some(line))
            {
                let message = message.trim();
                if !message.is_empty() {
                    snapshot.log_tail.push(message.to_owned());
                    if snapshot.log_tail.len() > 80 {
                        snapshot.log_tail.remove(0);
                    }
                }
            }
            snapshot.clone()
        };
        emit(app, &snapshot);
    }

    fn finish(&self, app: &AppHandle, id: &str, status: TransferStatus, error: Option<String>) {
        let snapshot = {
            let mut snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(snapshot) = snapshots.get_mut(id) else {
                return;
            };
            if snapshot.status == TransferStatus::Cancelled && status != TransferStatus::Cancelled {
                return;
            }
            snapshot.status = status;
            snapshot.finished_at = Some(unix_timestamp());
            if error.is_some() {
                snapshot.error = error;
            }
            snapshot.clone()
        };
        emit(app, &snapshot);
    }
}

fn emit(app: &AppHandle, snapshot: &TransferSnapshot) {
    let _ = app.emit("transfer:update", snapshot);
}

#[cfg(unix)]
async fn terminate_process(process_id: u32) {
    let _ = tokio::process::Command::new("kill")
        .arg("-TERM")
        .arg(process_id.to_string())
        .status()
        .await;
}

#[cfg(windows)]
async fn terminate_process(process_id: u32) {
    let _ = tokio::process::Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .status()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TransferDirection;

    #[test]
    fn new_transfer_starts_queued() {
        let request = TransferRequest {
            direction: TransferDirection::Download,
            operation: TransferOperation::Copy,
            source: "remote:file".into(),
            destination: "/tmp/file".into(),
            is_directory: false,
            extra_args: vec!["--checksum".into()],
            label: None,
        };
        let snapshot = TransferSnapshot::new("one".into(), &request);
        assert_eq!(snapshot.status, TransferStatus::Queued);
        assert_eq!(snapshot.bytes, 0);
        assert!(!snapshot.is_directory);
        assert_eq!(snapshot.extra_args, ["--checksum"]);
    }
}
