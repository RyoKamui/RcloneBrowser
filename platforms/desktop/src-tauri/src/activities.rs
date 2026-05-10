use std::{
    collections::HashMap,
    process::Stdio,
    sync::{Arc, Mutex},
};

use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::oneshot,
};
use uuid::Uuid;

use crate::{
    models::{ActivityKind, ActivitySnapshot, Settings, TransferStatus, unix_timestamp},
    rclone::RcloneClient,
};

#[derive(Clone, Default)]
pub struct ActivityManager {
    snapshots: Arc<Mutex<HashMap<String, ActivitySnapshot>>>,
    cancellations: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

pub struct ActivityContext {
    pub app: AppHandle,
    pub client: RcloneClient,
    pub settings: Settings,
    pub password: Option<String>,
}

pub struct MountSpec {
    pub source: String,
    pub destination: String,
    pub shared_with_me: bool,
    pub extra_args: Vec<String>,
}

pub struct StreamSpec {
    pub source: String,
    pub player_command: String,
    pub shared_with_me: bool,
}

impl ActivityManager {
    pub async fn start_mount(
        &self,
        context: ActivityContext,
        spec: MountSpec,
    ) -> Result<String, String> {
        let ActivityContext {
            app,
            client,
            settings,
            password,
        } = context;
        let MountSpec {
            source,
            destination,
            shared_with_me,
            extra_args,
        } = spec;
        if destination.trim().is_empty() {
            return Err("Choose a mount point first.".into());
        }
        #[cfg(not(windows))]
        if !std::path::Path::new(&destination).is_dir() {
            return Err("The mount point must be an existing directory.".into());
        }
        let mut arguments = vec!["mount".into(), source.clone(), destination.clone()];
        if shared_with_me {
            arguments.extend(["--drive-shared-with-me".into(), "--read-only".into()]);
        }
        arguments.extend(settings.mount_args.clone());
        arguments.extend(extra_args);
        let child = client
            .command(&settings, password.as_deref(), arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start rclone mount: {error}"))?;
        let id = self.insert(&app, ActivityKind::Mount, &source, &destination);
        let process_id = child.id();
        self.running(&app, &id);
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.clone(), cancel_tx);
        let destination_for_cancel = destination.clone();
        tauri::async_runtime::spawn(async move {
            if cancel_rx.await.is_ok() {
                unmount(&destination_for_cancel).await;
                if let Some(pid) = process_id {
                    terminate_process(pid).await;
                }
            }
        });
        self.watch_process(app, id.clone(), child);
        Ok(id)
    }

    pub async fn start_stream(
        &self,
        context: ActivityContext,
        spec: StreamSpec,
    ) -> Result<String, String> {
        let ActivityContext {
            app,
            client,
            settings,
            password,
        } = context;
        let StreamSpec {
            source,
            player_command,
            shared_with_me,
        } = spec;
        let parts = split_command(&player_command)?;
        let (program, player_args) = parts
            .split_first()
            .ok_or_else(|| "Enter a media player command in Settings.".to_owned())?;
        let mut player = tokio::process::Command::new(program)
            .args(player_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start media player '{program}': {error}"))?;
        let mut arguments = vec!["cat".into(), source.clone()];
        if shared_with_me {
            arguments.push("--drive-shared-with-me".into());
        }
        let mut rclone = match client
            .command(&settings, password.as_deref(), arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = player.kill().await;
                return Err(format!("Could not start rclone stream: {error}"));
            }
        };
        let id = self.insert(&app, ActivityKind::Stream, &source, &player_command);
        let rclone_pid = rclone.id();
        let player_pid = player.id();
        let rclone_output = rclone.stdout.take();
        let player_input = player.stdin.take();
        let player_stderr = player.stderr.take();
        self.running(&app, &id);
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.clone(), cancel_tx);
        tauri::async_runtime::spawn(async move {
            if cancel_rx.await.is_ok() {
                if let Some(pid) = rclone_pid {
                    terminate_process(pid).await;
                }
                if let Some(pid) = player_pid {
                    terminate_process(pid).await;
                }
            }
        });
        let manager = self.clone();
        let stream_id = id.clone();
        let event_app = app.clone();
        let stderr = rclone.stderr.take();
        tauri::async_runtime::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.log(&event_app, &stream_id, line);
                }
            }
        });
        let manager = self.clone();
        let stream_id = id.clone();
        let event_app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(stderr) = player_stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.log(&event_app, &stream_id, format!("player: {line}"));
                }
            }
        });
        let manager = self.clone();
        let stream_id = id.clone();
        tauri::async_runtime::spawn(async move {
            let copy = async {
                let mut output = rclone_output.ok_or("rclone stream had no output")?;
                let mut input = player_input.ok_or("media player had no input")?;
                tokio::io::copy(&mut output, &mut input)
                    .await
                    .map_err(|_| "Could not pipe the stream to the media player")?;
                Ok::<(), &str>(())
            };
            let copy_result = copy.await;
            let rclone_result = rclone.wait().await;
            let player_result = player.wait().await;
            manager.remove_cancellation(&stream_id);
            let error = copy_result
                .err()
                .map(str::to_owned)
                .or_else(|| process_error("rclone", rclone_result))
                .or_else(|| process_error("media player", player_result));
            manager.finish(&app, &stream_id, error);
        });
        Ok(id)
    }

    pub fn list(&self) -> Vec<ActivitySnapshot> {
        let mut values: Vec<_> = self
            .snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .cloned()
            .collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.started_at));
        values
    }

    pub fn cancel(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let sender = self
            .cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(id)
            .ok_or_else(|| "This activity is no longer running.".to_owned())?;
        let _ = sender.send(());
        self.set_status(app, id, TransferStatus::Cancelled, None);
        Ok(())
    }

    pub fn clear_finished(&self) {
        self.snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, item| {
                matches!(
                    item.status,
                    TransferStatus::Queued | TransferStatus::Running
                )
            });
    }

    pub fn has_running(&self) -> bool {
        !self
            .cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }

    pub fn cancel_all(&self, app: &AppHandle) {
        let ids: Vec<String> = self
            .cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.cancel(app, &id);
        }
    }

    fn insert(
        &self,
        app: &AppHandle,
        kind: ActivityKind,
        source: &str,
        destination: &str,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let snapshot = ActivitySnapshot {
            id: id.clone(),
            kind,
            source: source.into(),
            destination: destination.into(),
            status: TransferStatus::Queued,
            started_at: unix_timestamp(),
            finished_at: None,
            error: None,
            log_tail: Vec::new(),
        };
        self.snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.clone(), snapshot.clone());
        emit(app, &snapshot);
        id
    }

    fn watch_process(&self, app: AppHandle, id: String, mut child: tokio::process::Child) {
        let manager = self.clone();
        let log_id = id.clone();
        let event_app = app.clone();
        let stdout = child.stdout.take();
        tauri::async_runtime::spawn(async move {
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.log(&event_app, &log_id, line);
                }
            }
        });
        let manager = self.clone();
        let log_id = id.clone();
        let event_app = app.clone();
        let stderr = child.stderr.take();
        tauri::async_runtime::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.log(&event_app, &log_id, line);
                }
            }
        });
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = child.wait().await;
            manager.remove_cancellation(&id);
            manager.finish(&app, &id, process_error("rclone mount", result));
        });
    }

    fn running(&self, app: &AppHandle, id: &str) {
        self.set_status(app, id, TransferStatus::Running, None);
    }

    fn finish(&self, app: &AppHandle, id: &str, error: Option<String>) {
        let status = if error.is_some() {
            TransferStatus::Failed
        } else {
            TransferStatus::Completed
        };
        self.set_status(app, id, status, error);
    }

    fn set_status(&self, app: &AppHandle, id: &str, status: TransferStatus, error: Option<String>) {
        let snapshot = {
            let mut values = self
                .snapshots
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(snapshot) = values.get_mut(id) else {
                return;
            };
            if snapshot.status == TransferStatus::Cancelled && status != TransferStatus::Cancelled {
                return;
            }
            snapshot.status = status;
            snapshot.error = error;
            if !matches!(status, TransferStatus::Queued | TransferStatus::Running) {
                snapshot.finished_at = Some(unix_timestamp());
            }
            snapshot.clone()
        };
        emit(app, &snapshot);
    }

    fn log(&self, app: &AppHandle, id: &str, line: String) {
        let snapshot = {
            let mut values = self
                .snapshots
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(snapshot) = values.get_mut(id) else {
                return;
            };
            if !line.trim().is_empty() {
                snapshot.log_tail.push(line);
                if snapshot.log_tail.len() > 80 {
                    snapshot.log_tail.remove(0);
                }
            }
            snapshot.clone()
        };
        emit(app, &snapshot);
    }

    fn remove_cancellation(&self, id: &str) {
        self.cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(id);
    }
}

fn emit(app: &AppHandle, snapshot: &ActivitySnapshot) {
    let _ = app.emit("activity:update", snapshot);
}

fn process_error(
    name: &str,
    result: Result<std::process::ExitStatus, std::io::Error>,
) -> Option<String> {
    match result {
        Ok(status) if status.success() => None,
        Ok(status) => Some(format!("{name} exited with {status}")),
        Err(error) => Some(format!("Could not wait for {name}: {error}")),
    }
}

fn split_command(value: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quote != Some('\'') {
            escaped = true;
        } else if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                current.push(character);
            }
        } else if character.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if escaped || quote.is_some() {
        return Err("The player command contains an unfinished quote or escape.".into());
    }
    if !current.is_empty() {
        parts.push(current);
    }
    Ok(parts)
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

#[cfg(target_os = "macos")]
async fn unmount(destination: &str) {
    let result = tokio::process::Command::new("diskutil")
        .args(["unmount", destination])
        .status()
        .await;
    if !matches!(result, Ok(status) if status.success()) {
        let _ = tokio::process::Command::new("umount")
            .arg(destination)
            .status()
            .await;
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
async fn unmount(destination: &str) {
    let result = tokio::process::Command::new("fusermount3")
        .args(["-u", destination])
        .status()
        .await;
    if !matches!(result, Ok(status) if status.success()) {
        let _ = tokio::process::Command::new("fusermount")
            .args(["-u", destination])
            .status()
            .await;
    }
}

#[cfg(windows)]
async fn unmount(_destination: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_player_commands_without_a_shell() {
        assert_eq!(
            split_command("/Applications/Test\\ Player --title 'Cloud video' -").unwrap(),
            vec!["/Applications/Test Player", "--title", "Cloud video", "-"]
        );
        assert!(split_command("player 'unfinished").is_err());
    }
}
