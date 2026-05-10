mod activities;
mod models;
mod rclone;
mod settings;
mod tasks;
mod transfers;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

use activities::{ActivityContext, ActivityManager, MountSpec, StreamSpec};
use models::{
    ActivitySnapshot, Bootstrap, ConfigProvider, ConfigQuestion, DirectorySummary, Entry,
    ExportFormat, ExportOptions, RcloneUpdateInfo, Remote, SavedTask, Settings, TransferDirection,
    TransferOperation, TransferRequest, TransferSnapshot, UpdateStatus,
};
use rclone::{ExportRequest, RcloneClient, browser_join, browser_target, resolved_path};
use settings::{SettingsStore, legacy_settings_paths};
use tasks::{TaskStore, legacy_task_paths};
use tauri::{
    AppHandle, Emitter, Manager, State, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use transfers::TransferManager;

#[derive(Clone)]
struct AppState {
    settings: SettingsStore,
    rclone: RcloneClient,
    transfers: TransferManager,
    activities: ActivityManager,
    tasks: TaskStore,
    config_password: Arc<RwLock<Option<String>>>,
    pending_config_remotes: Arc<RwLock<HashSet<String>>>,
    portable: bool,
    data_directory: PathBuf,
    quitting: Arc<AtomicBool>,
}

impl AppState {
    fn password(&self) -> Option<String> {
        self.config_password
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn mark_pending_config(&self, name: &str) {
        self.pending_config_remotes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(name.to_owned());
    }

    fn take_pending_config(&self, name: &str) -> bool {
        self.pending_config_remotes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(name)
    }

    fn is_pending_config(&self, name: &str) -> bool {
        self.pending_config_remotes
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(name)
    }

    fn take_all_pending_configs(&self) -> Vec<String> {
        self.pending_config_remotes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
            .collect()
    }

    fn has_pending_configs(&self) -> bool {
        !self
            .pending_config_remotes
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }

    fn has_running_work(&self) -> bool {
        self.transfers.has_running() || self.activities.has_running()
    }
}

#[tauri::command]
async fn bootstrap(state: State<'_, AppState>) -> Result<Bootstrap, String> {
    let settings = state.settings.get();
    let password = state.password();
    let mut rclone = state.rclone.status(&settings, password.as_deref()).await;
    let remotes = if rclone.available {
        match state
            .rclone
            .list_remotes(&settings, password.as_deref())
            .await
        {
            Ok(remotes) => remotes,
            Err(error) => {
                rclone.available = false;
                rclone.error = Some(format!("Could not read rclone remotes: {error}"));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    Ok(Bootstrap {
        app_version: env!("CARGO_PKG_VERSION").into(),
        settings,
        rclone,
        remotes,
        transfers: state.transfers.list(),
        activities: state.activities.list(),
        tasks: state.tasks.list(),
        portable: state.portable,
        data_directory: state.data_directory.to_string_lossy().into_owned(),
        home_directory: home_directory().to_string_lossy().into_owned(),
    })
}

fn home_directory() -> PathBuf {
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(path);
    }
    #[cfg(windows)]
    if let (Some(drive), Some(path)) = (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH"))
    {
        let mut value = drive;
        value.push(path);
        return PathBuf::from(value);
    }
    #[cfg(not(windows))]
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(std::path::MAIN_SEPARATOR_STR))
}

#[tauri::command]
async fn list_remotes(state: State<'_, AppState>) -> Result<Vec<Remote>, String> {
    state
        .rclone
        .list_remotes(&state.settings.get(), state.password().as_deref())
        .await
}

#[tauri::command]
async fn list_config_providers(state: State<'_, AppState>) -> Result<Vec<ConfigProvider>, String> {
    state
        .rclone
        .config_providers(&state.settings.get(), state.password().as_deref())
        .await
}

#[tauri::command]
async fn start_location_config(
    state: State<'_, AppState>,
    name: String,
    provider: String,
) -> Result<ConfigQuestion, String> {
    let question = state
        .rclone
        .start_config_create(
            &state.settings.get(),
            state.password().as_deref(),
            &name,
            &provider,
        )
        .await?;
    if !question.state.is_empty() || question.option.is_some() {
        state.mark_pending_config(name.trim());
    }
    Ok(question)
}

#[tauri::command]
async fn continue_location_config(
    state: State<'_, AppState>,
    name: String,
    provider: String,
    session_state: String,
    result: String,
) -> Result<ConfigQuestion, String> {
    if !state.is_pending_config(name.trim()) {
        return Err("This location setup is no longer active.".into());
    }
    let question = state
        .rclone
        .continue_config_create(
            &state.settings.get(),
            state.password().as_deref(),
            &name,
            &provider,
            &session_state,
            &result,
        )
        .await?;
    if question.state.is_empty() && question.option.is_none() {
        state.take_pending_config(name.trim());
    }
    Ok(question)
}

#[tauri::command]
async fn cancel_location_config(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let name = name.trim();
    if !state.take_pending_config(name) {
        return Ok(());
    }
    if let Err(error) = state
        .rclone
        .delete_config_remote(&state.settings.get(), state.password().as_deref(), name)
        .await
    {
        state.mark_pending_config(name);
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
async fn list_entries(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    shared_with_me: bool,
) -> Result<Vec<Entry>, String> {
    state
        .rclone
        .list_entries(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &path,
            shared_with_me,
        )
        .await
}

#[tauri::command]
async fn create_folder(
    state: State<'_, AppState>,
    remote: String,
    parent: String,
    name: String,
) -> Result<(), String> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') {
        return Err("Folder name must be a single non-empty path component.".into());
    }
    state
        .rclone
        .create_folder(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &browser_join(&remote, &parent, name.trim()),
        )
        .await
}

#[tauri::command]
async fn rename_entry(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    new_name: String,
) -> Result<(), String> {
    state
        .rclone
        .rename_entry(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &path,
            &new_name,
        )
        .await
}

#[tauri::command]
async fn move_entry(
    state: State<'_, AppState>,
    remote: String,
    source: String,
    destination: String,
) -> Result<(), String> {
    state
        .rclone
        .move_entry(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &source,
            &destination,
        )
        .await
}

#[tauri::command]
async fn delete_entry(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    is_dir: bool,
) -> Result<(), String> {
    state
        .rclone
        .delete_entry(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &path,
            is_dir,
        )
        .await
}

#[tauri::command]
async fn get_public_link(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    shared_with_me: bool,
) -> Result<String, String> {
    state
        .rclone
        .public_link(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &path,
            shared_with_me,
        )
        .await
}

#[tauri::command]
async fn get_directory_size(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    shared_with_me: bool,
) -> Result<DirectorySummary, String> {
    state
        .rclone
        .directory_size(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &path,
            shared_with_me,
        )
        .await
}

#[tauri::command]
async fn get_directory_tree(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    shared_with_me: bool,
) -> Result<String, String> {
    state
        .rclone
        .directory_tree(
            &state.settings.get(),
            state.password().as_deref(),
            &remote,
            &path,
            shared_with_me,
        )
        .await
}

#[tauri::command]
async fn export_listing(
    state: State<'_, AppState>,
    remote: String,
    path: String,
    shared_with_me: bool,
    destination: String,
    format: ExportFormat,
    options: ExportOptions,
) -> Result<u64, String> {
    state
        .rclone
        .export_listing(
            &state.settings.get(),
            state.password().as_deref(),
            ExportRequest {
                remote: &remote,
                path: &path,
                shared_with_me,
                destination: Path::new(&destination),
                format,
                options: &options,
            },
        )
        .await
}

#[tauri::command]
fn copy_command(
    state: State<'_, AppState>,
    operation: TransferOperation,
    source: String,
    destination: String,
    is_directory: bool,
    extra_args: Vec<String>,
) -> String {
    let settings = state.settings.get();
    build_copy_command(
        &settings,
        operation,
        &source,
        &destination,
        is_directory,
        &extra_args,
    )
}

fn build_copy_command(
    settings: &Settings,
    operation: TransferOperation,
    source: &str,
    destination: &str,
    is_directory: bool,
    extra_args: &[String],
) -> String {
    let mut parts = vec![settings.rclone_path.clone()];
    if let Some(config) = settings
        .config_path
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        parts.extend(["--config".into(), config.clone()]);
    }
    parts.extend(settings.advanced_args.clone());
    parts.push(
        match (operation, is_directory) {
            (TransferOperation::Copy, true) => "copy",
            (TransferOperation::Copy, false) => "copyto",
            (TransferOperation::Move, true) => "move",
            (TransferOperation::Move, false) => "moveto",
            (TransferOperation::Sync, _) => "sync",
        }
        .into(),
    );
    if is_directory {
        parts.push("--create-empty-src-dirs".into());
    }
    parts.extend(extra_args.iter().cloned());
    parts.extend([source.to_owned(), destination.to_owned()]);
    parts
        .iter()
        .map(|value| shell_quote(value))
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let tray_visible = settings.always_show_tray || settings.close_to_tray;
    state.settings.save(settings)?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_visible(tray_visible);
    }
    Ok(())
}

#[tauri::command]
fn set_config_password(state: State<'_, AppState>, password: String) {
    *state
        .config_password
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
        (!password.is_empty()).then_some(password);
}

#[tauri::command]
fn open_rclone_config(state: State<'_, AppState>) -> Result<(), String> {
    launch_rclone_terminal(&state.settings.get(), &["config".into()])
}

#[tauri::command]
fn reconnect_remote(state: State<'_, AppState>, remote: String) -> Result<(), String> {
    if remote == "__local__" {
        return Err("The local filesystem does not require authentication.".into());
    }
    launch_rclone_terminal(
        &state.settings.get(),
        &[
            "config".into(),
            "reconnect".into(),
            format!("{}:", remote.trim_end_matches(':')),
        ],
    )
}

#[tauri::command]
async fn check_rclone_update(state: State<'_, AppState>) -> Result<RcloneUpdateInfo, String> {
    state
        .rclone
        .check_rclone_update(&state.settings.get(), state.password().as_deref())
        .await
}

#[tauri::command]
fn open_rclone_download(channel: String, version: String) -> Result<(), String> {
    let url = rclone_download_url(&channel, &version)?;
    #[cfg(target_os = "macos")]
    Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|error| format!("Could not open the download page: {error}"))?;
    #[cfg(windows)]
    Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn()
        .map_err(|error| format!("Could not open the download page: {error}"))?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|error| format!("Could not open the download page: {error}"))?;
    Ok(())
}

fn rclone_download_url(channel: &str, version: &str) -> Result<String, String> {
    if version.is_empty()
        || !version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-+_".contains(character))
    {
        return Err("The rclone version is not valid.".into());
    }
    let base = match channel {
        "stable" => "https://downloads.rclone.org/v",
        "beta" => "https://beta.rclone.org/v",
        _ => return Err("The rclone release channel is not valid.".into()),
    };
    Ok(format!("{base}{version}"))
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
}

#[tauri::command]
async fn check_app_update() -> Result<UpdateStatus, String> {
    let release = reqwest::Client::new()
        .get("https://api.github.com/repos/kapitainsky/RcloneBrowser/releases/latest")
        .header(reqwest::header::USER_AGENT, "Rclone-Browser-Rust")
        .send()
        .await
        .map_err(|error| format!("Could not check for application updates: {error}"))?
        .error_for_status()
        .map_err(|error| format!("The update service returned an error: {error}"))?
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("The update response was invalid: {error}"))?;
    let current = env!("CARGO_PKG_VERSION");
    let latest = release.tag_name.trim_start_matches('v');
    let available = match (
        semver::Version::parse(current),
        semver::Version::parse(latest),
    ) {
        (Ok(current), Ok(latest)) => latest > current,
        _ => current != latest,
    };
    Ok(UpdateStatus {
        current_version: current.into(),
        latest_version: release.tag_name,
        available,
        release_url: release.html_url,
    })
}

#[tauri::command]
async fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    entry: Entry,
    destination_directory: String,
    shared_with_me: bool,
    extra_args: Vec<String>,
) -> Result<String, String> {
    let destination = PathBuf::from(destination_directory).join(&entry.name);
    let mut arguments = state.settings.get().default_download_args;
    arguments.extend(extra_args);
    if shared_with_me {
        arguments.push("--drive-shared-with-me".into());
    }
    let request = TransferRequest {
        direction: TransferDirection::Download,
        operation: TransferOperation::Copy,
        source: browser_target(&remote, &entry.path),
        destination: destination.to_string_lossy().into_owned(),
        is_directory: entry.is_dir,
        extra_args: arguments,
        label: Some(format!("Download {}", entry.name)),
    };
    start_transfer_inner(app, &state, request).await
}

#[tauri::command]
async fn start_upload(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    path: String,
    local_paths: Vec<String>,
    shared_with_me: bool,
    extra_args: Vec<String>,
) -> Result<Vec<String>, String> {
    let settings = state.settings.get();
    let mut ids = Vec::with_capacity(local_paths.len());
    for local_path in local_paths {
        let local = PathBuf::from(&local_path);
        let metadata = std::fs::metadata(&local)
            .map_err(|error| format!("Could not inspect '{}': {error}", local.display()))?;
        let name = local
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("'{}' has no valid file name", local.display()))?;
        let mut arguments = settings.default_upload_args.clone();
        arguments.extend(extra_args.clone());
        if shared_with_me {
            arguments.push("--drive-shared-with-me".into());
        }
        let request = TransferRequest {
            direction: TransferDirection::Upload,
            operation: TransferOperation::Copy,
            source: local_path,
            destination: browser_target(&remote, &browser_join(&remote, &path, name)),
            is_directory: metadata.is_dir(),
            extra_args: arguments,
            label: Some(format!("Upload {name}")),
        };
        ids.push(start_transfer_inner(app.clone(), &state, request).await?);
    }
    Ok(ids)
}

#[tauri::command]
async fn start_custom_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TransferRequest,
) -> Result<String, String> {
    if request.source.trim().is_empty() || request.destination.trim().is_empty() {
        return Err("Both source and destination are required.".into());
    }
    start_transfer_inner(app, &state, request).await
}

async fn start_transfer_inner(
    app: AppHandle,
    state: &State<'_, AppState>,
    request: TransferRequest,
) -> Result<String, String> {
    state
        .transfers
        .start(
            app,
            state.rclone.clone(),
            state.settings.get(),
            state.password(),
            request,
        )
        .await
}

#[tauri::command]
fn list_transfers(state: State<'_, AppState>) -> Vec<TransferSnapshot> {
    state.transfers.list()
}

#[tauri::command]
fn cancel_transfer(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.transfers.cancel(&app, &id)
}

#[tauri::command]
fn clear_finished_transfers(state: State<'_, AppState>) {
    state.transfers.clear_finished();
    state.activities.clear_finished();
}

#[tauri::command]
async fn start_mount(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    path: String,
    destination: String,
    shared_with_me: bool,
    extra_args: Vec<String>,
) -> Result<String, String> {
    state
        .activities
        .start_mount(
            ActivityContext {
                app,
                client: state.rclone.clone(),
                settings: state.settings.get(),
                password: state.password(),
            },
            MountSpec {
                source: browser_target(&remote, &path),
                destination,
                shared_with_me,
                extra_args,
            },
        )
        .await
}

#[tauri::command]
async fn start_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    path: String,
    player_command: Option<String>,
    shared_with_me: bool,
) -> Result<String, String> {
    let settings = state.settings.get();
    state
        .activities
        .start_stream(
            ActivityContext {
                app,
                client: state.rclone.clone(),
                settings: settings.clone(),
                password: state.password(),
            },
            StreamSpec {
                source: browser_target(&remote, &path),
                player_command: player_command.unwrap_or(settings.stream_command),
                shared_with_me,
            },
        )
        .await
}

#[tauri::command]
fn list_activities(state: State<'_, AppState>) -> Vec<ActivitySnapshot> {
    state.activities.list()
}

#[tauri::command]
fn cancel_activity(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.activities.cancel(&app, &id)
}

#[tauri::command]
fn list_tasks(state: State<'_, AppState>) -> Vec<SavedTask> {
    state.tasks.list()
}

#[tauri::command]
fn save_task(state: State<'_, AppState>, task: SavedTask) -> Result<SavedTask, String> {
    state.tasks.save(task)
}

#[tauri::command]
fn delete_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.tasks.delete(&id)
}

#[tauri::command]
async fn run_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    dry_run: bool,
) -> Result<String, String> {
    let task = state
        .tasks
        .get(&id)
        .ok_or_else(|| "The saved task no longer exists.".to_owned())?;
    let all_arguments = task.arguments(dry_run);
    let extra_args = all_arguments[1..all_arguments.len() - 2].to_vec();
    let request = TransferRequest {
        direction: task.direction,
        operation: task.operation,
        source: task.source,
        destination: task.destination,
        is_directory: task.is_directory,
        extra_args,
        label: Some(task.description),
    };
    start_transfer_inner(app, &state, request).await
}

#[tauri::command]
async fn quit_app(app: AppHandle, state: State<'_, AppState>, force: bool) -> Result<(), String> {
    if state.has_running_work() && !force {
        return Err("Transfers, mounts, or streams are still running.".into());
    }
    if force {
        state.transfers.cancel_all(&app);
        state.activities.cancel_all(&app);
    }
    state.quitting.store(true, Ordering::Release);
    cleanup_pending_configs(state.inner()).await;
    app.exit(0);
    Ok(())
}

async fn cleanup_pending_configs(state: &AppState) {
    let names = state.take_all_pending_configs();
    if names.is_empty() {
        return;
    }
    let settings = state.settings.get();
    let password = state.password();
    for name in names {
        let _ = state
            .rclone
            .delete_config_remote(&settings, password.as_deref(), &name)
            .await;
    }
}

fn setup_tray(app: &tauri::App, visible: bool) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Rclone Browser", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("Rclone Browser")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "quit" => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(event, tauri::tray::TrayIconEvent::Click { .. }) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    tray.set_visible(visible)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn request_quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.has_running_work() {
        show_main_window(app);
        let _ = app.emit("app:quit-requested", ());
    } else {
        state.quitting.store(true, Ordering::Release);
        let owned_state = state.inner().clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            cleanup_pending_configs(&owned_state).await;
            app.exit(0);
        });
    }
}

fn resolve_data_directory(default: PathBuf) -> (PathBuf, bool) {
    #[cfg(any(target_os = "macos", windows))]
    if let Ok(executable) = std::env::current_exe() {
        #[cfg(target_os = "macos")]
        if let Some((bundle, parent)) = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(|bundle| bundle.parent().map(|parent| (bundle, parent)))
        {
            let stem = bundle
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("rclone-browser");
            let markers = [
                parent.join(format!("{stem}.ini")),
                parent.join("rclone-browser.ini"),
            ];
            if markers.iter().any(|marker| marker.is_file()) {
                return (parent.to_path_buf(), true);
            }
        }
        #[cfg(windows)]
        if let Some(parent) = executable.parent() {
            let marker = executable.with_extension("ini");
            if marker.is_file() {
                return (parent.to_path_buf(), true);
            }
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|xdg| std::env::var_os("APPIMAGE").map(PathBuf::from).as_deref() == xdg.parent())
    {
        return (xdg.join("rclone-browser"), true);
    }
    (default, false)
}

fn launch_rclone_terminal(settings: &Settings, arguments: &[String]) -> Result<(), String> {
    let mut command_parts = Vec::new();
    if settings.use_proxy {
        push_env(&mut command_parts, "HTTP_PROXY", &settings.http_proxy);
        push_env(&mut command_parts, "HTTPS_PROXY", &settings.https_proxy);
        push_env(&mut command_parts, "NO_PROXY", &settings.no_proxy);
    }
    command_parts.push(shell_quote(
        &resolved_path(settings, &settings.rclone_path).to_string_lossy(),
    ));
    if let Some(config) = settings
        .config_path
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        command_parts.extend([
            "--config".into(),
            shell_quote(&resolved_path(settings, config).to_string_lossy()),
        ]);
    }
    command_parts.extend(
        settings
            .advanced_args
            .iter()
            .map(|value| shell_quote(value)),
    );
    command_parts.extend(arguments.iter().map(|value| shell_quote(value)));
    let command_line = command_parts.join(" ");
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Terminal\" to do script \"{}; printf '\\\\nPress return to close…'; read _\"",
            command_line.replace('\\', "\\\\").replace('"', "\\\"")
        );
        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|error| format!("Could not open Terminal: {error}"))?;
    }
    #[cfg(windows)]
    Command::new("cmd")
        .args([
            "/C",
            "start",
            "rclone configuration",
            "cmd",
            "/K",
            &command_line,
        ])
        .spawn()
        .map_err(|error| format!("Could not open a command prompt: {error}"))?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("x-terminal-emulator")
        .args([
            "-e",
            "sh",
            "-lc",
            &format!("{command_line}; printf '\\nPress return to close…'; read _"),
        ])
        .spawn()
        .map_err(|error| format!("Could not open a terminal: {error}"))?;
    Ok(())
}

fn push_env(parts: &mut Vec<String>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        parts.push(format!("{key}={}", shell_quote(value.trim())));
    }
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./:@%+=,-".contains(&byte))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let default_dir = app.path().app_config_dir().map_err(|error| {
                format!("Could not find the app configuration directory: {error}")
            })?;
            let (data_directory, portable) = resolve_data_directory(default_dir);
            let settings = SettingsStore::open(
                &data_directory,
                portable,
                &legacy_settings_paths(&data_directory),
            )?;
            let tasks = TaskStore::open(&data_directory, &legacy_task_paths(&data_directory))?;
            let tray_visible = settings.get().always_show_tray || settings.get().close_to_tray;
            app.manage(AppState {
                settings,
                rclone: RcloneClient,
                transfers: TransferManager::default(),
                activities: ActivityManager::default(),
                tasks,
                config_password: Arc::new(RwLock::new(None)),
                pending_config_remotes: Arc::new(RwLock::new(HashSet::new())),
                portable,
                data_directory,
                quitting: Arc::new(AtomicBool::new(false)),
            });
            setup_tray(app, tray_visible)?;
            show_main_window(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.quitting.load(Ordering::Acquire) {
                    return;
                }
                if state.settings.get().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else if state.has_running_work() {
                    api.prevent_close();
                    let _ = window.emit("app:quit-requested", ());
                } else if state.has_pending_configs() {
                    api.prevent_close();
                    state.quitting.store(true, Ordering::Release);
                    let owned_state = state.inner().clone();
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        cleanup_pending_configs(&owned_state).await;
                        app.exit(0);
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            list_remotes,
            list_config_providers,
            start_location_config,
            continue_location_config,
            cancel_location_config,
            list_entries,
            create_folder,
            rename_entry,
            move_entry,
            delete_entry,
            get_public_link,
            get_directory_size,
            get_directory_tree,
            export_listing,
            copy_command,
            save_settings,
            set_config_password,
            open_rclone_config,
            reconnect_remote,
            check_rclone_update,
            open_rclone_download,
            check_app_update,
            start_download,
            start_upload,
            start_custom_transfer,
            list_transfers,
            cancel_transfer,
            clear_finished_transfers,
            start_mount,
            start_stream,
            list_activities,
            cancel_activity,
            list_tasks,
            save_task,
            delete_task,
            run_task,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Rclone Browser");
    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main_window(app);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_commands_are_escaped_for_display_and_terminal_use() {
        assert_eq!(shell_quote("rclone"), "rclone");
        assert_eq!(shell_quote("a folder/it's"), "'a folder/it'\\''s'");
        let settings = Settings {
            rclone_path: "/Applications/rclone tool".into(),
            config_path: Some("/tmp/cloud config.conf".into()),
            ..Default::default()
        };
        let command = build_copy_command(
            &settings,
            TransferOperation::Copy,
            "remote:file.txt",
            "/tmp/new file.txt",
            false,
            &["--checksum".into()],
        );
        assert!(command.contains("copyto --checksum"));
        assert!(command.contains("'/tmp/new file.txt'"));
    }

    #[test]
    fn rclone_download_pages_are_fixed_to_official_hosts() {
        assert_eq!(
            rclone_download_url("stable", "1.75.0").unwrap(),
            "https://downloads.rclone.org/v1.75.0"
        );
        assert_eq!(
            rclone_download_url("beta", "1.76.0-beta.10147.f0b210a88").unwrap(),
            "https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88"
        );
        assert!(rclone_download_url("stable", "1.75.0&bad").is_err());
        assert!(rclone_download_url("nightly", "1.75.0").is_err());
    }
}
