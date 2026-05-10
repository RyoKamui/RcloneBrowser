use serde::{Deserialize, Serialize};

pub use rclone_browser_shared::RcloneUpdateInfo;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub rclone_path: String,
    pub config_path: Option<String>,
    pub default_download_dir: Option<String>,
    pub default_upload_dir: Option<String>,
    pub default_download_args: Vec<String>,
    pub default_upload_args: Vec<String>,
    pub show_hidden: bool,
    pub show_folder_icons: bool,
    pub show_file_icons: bool,
    pub alternating_rows: bool,
    pub icon_size: IconSize,
    pub confirm_delete: bool,
    pub theme: Theme,
    pub advanced_args: Vec<String>,
    pub stream_command: String,
    pub mount_args: Vec<String>,
    pub close_to_tray: bool,
    pub always_show_tray: bool,
    pub notify_finished_transfers: bool,
    pub check_app_updates: bool,
    pub check_rclone_updates: bool,
    pub use_proxy: bool,
    pub http_proxy: String,
    pub https_proxy: String,
    pub no_proxy: String,
    pub export_options: ExportOptions,
    pub dual_pane: bool,
    pub show_transfer_shelf: bool,
    pub compact_rows: bool,
    #[serde(skip)]
    pub portable_base: Option<std::path::PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rclone_path: "rclone".into(),
            config_path: None,
            default_download_dir: None,
            default_upload_dir: None,
            default_download_args: Vec::new(),
            default_upload_args: Vec::new(),
            show_hidden: true,
            show_folder_icons: true,
            show_file_icons: true,
            alternating_rows: true,
            icon_size: IconSize::Medium,
            confirm_delete: true,
            theme: Theme::System,
            advanced_args: Vec::new(),
            stream_command: "mpv -".into(),
            mount_args: vec!["--vfs-cache-mode".into(), "writes".into()],
            close_to_tray: false,
            always_show_tray: false,
            notify_finished_transfers: true,
            check_app_updates: true,
            check_rclone_updates: true,
            use_proxy: false,
            http_proxy: String::new(),
            https_proxy: String::new(),
            no_proxy: String::new(),
            export_options: ExportOptions::default(),
            dual_pane: true,
            show_transfer_shelf: true,
            compact_rows: true,
            portable_base: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IconSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    pub one_file_system: bool,
    pub min_size: String,
    pub min_age: String,
    pub max_age: String,
    pub max_depth: u32,
    pub excludes: Vec<String>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub app_version: String,
    pub settings: Settings,
    pub rclone: RcloneStatus,
    pub remotes: Vec<Remote>,
    pub transfers: Vec<TransferSnapshot>,
    pub activities: Vec<ActivitySnapshot>,
    pub tasks: Vec<SavedTask>,
    pub portable: bool,
    pub data_directory: String,
    pub home_directory: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RcloneStatus {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    #[serde(rename = "type")]
    pub remote_type: String,
    pub description: String,
    pub is_local: bool,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all(serialize = "camelCase", deserialize = "PascalCase"))]
pub struct ConfigProvider {
    pub name: String,
    pub description: String,
    pub prefix: String,
    #[serde(default)]
    pub hide: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all(serialize = "camelCase", deserialize = "PascalCase"))]
pub struct ConfigExample {
    pub value: String,
    pub help: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    default,
    rename_all(serialize = "camelCase", deserialize = "PascalCase")
)]
pub struct ConfigOption {
    pub name: String,
    pub help: String,
    pub default_str: String,
    pub value_str: String,
    pub required: bool,
    pub is_password: bool,
    pub exclusive: bool,
    pub sensitive: bool,
    #[serde(rename(serialize = "optionType", deserialize = "Type"))]
    pub option_type: String,
    pub examples: Vec<ConfigExample>,
}

impl Default for ConfigOption {
    fn default() -> Self {
        Self {
            name: String::new(),
            help: String::new(),
            default_str: String::new(),
            value_str: String::new(),
            required: false,
            is_password: false,
            exclusive: false,
            sensitive: false,
            option_type: "string".into(),
            examples: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    default,
    rename_all(serialize = "camelCase", deserialize = "PascalCase")
)]
pub struct ConfigQuestion {
    pub state: String,
    pub option: Option<ConfigOption>,
    pub error: String,
    pub result: String,
}

impl Default for Remote {
    fn default() -> Self {
        Self {
            name: String::new(),
            remote_type: "unknown".into(),
            description: String::new(),
            is_local: false,
            display_name: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub mod_time: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Upload,
    Download,
    Copy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferOperation {
    #[default]
    Copy,
    Move,
    Sync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TransferRequest {
    pub direction: TransferDirection,
    pub operation: TransferOperation,
    pub source: String,
    pub destination: String,
    pub is_directory: bool,
    pub extra_args: Vec<String>,
    pub label: Option<String>,
}

impl Default for TransferRequest {
    fn default() -> Self {
        Self {
            direction: TransferDirection::Copy,
            operation: TransferOperation::Copy,
            source: String::new(),
            destination: String::new(),
            is_directory: true,
            extra_args: Vec::new(),
            label: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSnapshot {
    pub id: String,
    pub direction: TransferDirection,
    pub operation: TransferOperation,
    pub label: Option<String>,
    pub source: String,
    pub destination: String,
    pub is_directory: bool,
    pub extra_args: Vec<String>,
    pub status: TransferStatus,
    pub bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<f64>,
    pub eta_seconds: Option<f64>,
    pub checks: u64,
    pub total_checks: Option<u64>,
    pub files_transferred: u64,
    pub total_files: Option<u64>,
    pub errors: u64,
    pub elapsed_seconds: Option<f64>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
    pub log_tail: Vec<String>,
}

impl TransferSnapshot {
    pub fn new(id: String, request: &TransferRequest) -> Self {
        Self {
            id,
            direction: request.direction,
            operation: request.operation,
            label: request.label.clone(),
            source: request.source.clone(),
            destination: request.destination.clone(),
            is_directory: request.is_directory,
            extra_args: request.extra_args.clone(),
            status: TransferStatus::Queued,
            bytes: 0,
            total_bytes: None,
            speed: None,
            eta_seconds: None,
            checks: 0,
            total_checks: None,
            files_transferred: 0,
            total_files: None,
            errors: 0,
            elapsed_seconds: None,
            started_at: unix_timestamp(),
            finished_at: None,
            error: None,
            log_tail: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    Mount,
    Stream,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySnapshot {
    pub id: String,
    pub kind: ActivityKind,
    pub source: String,
    pub destination: String,
    pub status: TransferStatus,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
    pub log_tail: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncDeleteMode {
    During,
    After,
    Before,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompareMode {
    SizeAndModTime,
    Checksum,
    IgnoreSize,
    SizeOnly,
    ChecksumIgnoreSize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedTask {
    pub id: String,
    pub description: String,
    pub direction: TransferDirection,
    pub operation: TransferOperation,
    pub source: String,
    pub destination: String,
    pub is_directory: bool,
    pub sync_delete_mode: Option<SyncDeleteMode>,
    pub update: bool,
    pub ignore_existing: bool,
    pub compare_mode: CompareMode,
    pub one_file_system: bool,
    pub no_update_modtime: bool,
    pub transfers: u16,
    pub checkers: u16,
    pub bandwidth: String,
    pub min_size: String,
    pub min_age: String,
    pub max_age: String,
    pub max_depth: u32,
    pub connect_timeout_seconds: u32,
    pub idle_timeout_seconds: u32,
    pub retries: u16,
    pub low_level_retries: u16,
    pub delete_excluded: bool,
    pub excludes: Vec<String>,
    pub extra_args: Vec<String>,
    pub shared_with_me: bool,
}

impl Default for SavedTask {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: String::new(),
            direction: TransferDirection::Copy,
            operation: TransferOperation::Copy,
            source: String::new(),
            destination: String::new(),
            is_directory: true,
            sync_delete_mode: None,
            update: false,
            ignore_existing: false,
            compare_mode: CompareMode::SizeAndModTime,
            one_file_system: false,
            no_update_modtime: false,
            transfers: 4,
            checkers: 8,
            bandwidth: String::new(),
            min_size: String::new(),
            min_age: String::new(),
            max_age: String::new(),
            max_depth: 0,
            connect_timeout_seconds: 60,
            idle_timeout_seconds: 300,
            retries: 3,
            low_level_retries: 10,
            delete_excluded: false,
            excludes: Vec::new(),
            extra_args: Vec::new(),
            shared_with_me: false,
        }
    }
}

impl SavedTask {
    pub fn arguments(&self, dry_run: bool) -> Vec<String> {
        let mut args = vec![
            match self.operation {
                TransferOperation::Copy => "copy",
                TransferOperation::Move => "move",
                TransferOperation::Sync => "sync",
            }
            .into(),
        ];
        if dry_run {
            args.push("--dry-run".into());
        }
        if self.operation == TransferOperation::Sync
            && let Some(mode) = self.sync_delete_mode
        {
            args.push(
                match mode {
                    SyncDeleteMode::During => "--delete-during",
                    SyncDeleteMode::After => "--delete-after",
                    SyncDeleteMode::Before => "--delete-before",
                }
                .into(),
            );
        }
        if self.update {
            args.push("--update".into());
        }
        if self.ignore_existing {
            args.push("--ignore-existing".into());
        }
        match self.compare_mode {
            CompareMode::Checksum => args.push("--checksum".into()),
            CompareMode::IgnoreSize => args.push("--ignore-size".into()),
            CompareMode::SizeOnly => args.push("--size-only".into()),
            CompareMode::ChecksumIgnoreSize => {
                args.extend(["--checksum".into(), "--ignore-size".into()]);
            }
            CompareMode::SizeAndModTime => {}
        }
        if self.one_file_system {
            args.push("--one-file-system".into());
        }
        if self.no_update_modtime {
            args.push("--no-update-modtime".into());
        }
        args.extend(["--transfers".into(), self.transfers.to_string()]);
        args.extend(["--checkers".into(), self.checkers.to_string()]);
        push_option(&mut args, "--bwlimit", &self.bandwidth);
        push_option(&mut args, "--min-size", &self.min_size);
        push_option(&mut args, "--min-age", &self.min_age);
        push_option(&mut args, "--max-age", &self.max_age);
        if self.max_depth > 0 {
            args.extend(["--max-depth".into(), self.max_depth.to_string()]);
        }
        args.extend([
            "--contimeout".into(),
            format!("{}s", self.connect_timeout_seconds),
            "--timeout".into(),
            format!("{}s", self.idle_timeout_seconds),
            "--retries".into(),
            self.retries.to_string(),
            "--low-level-retries".into(),
            self.low_level_retries.to_string(),
        ]);
        if self.delete_excluded {
            args.push("--delete-excluded".into());
        }
        for exclude in self.excludes.iter().filter(|value| !value.is_empty()) {
            args.extend(["--exclude".into(), exclude.clone()]);
        }
        args.extend(self.extra_args.clone());
        if self.shared_with_me {
            args.push("--drive-shared-with-me".into());
        }
        args.extend([
            "--stats".into(),
            "1s".into(),
            "--stats-file-name-length".into(),
            "0".into(),
            self.source.clone(),
            self.destination.clone(),
        ]);
        args
    }
}

fn push_option(args: &mut Vec<String>, flag: &str, value: &str) {
    if !value.trim().is_empty() {
        args.extend([flag.into(), value.trim().into()]);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySummary {
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub release_url: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Txt,
    Csv,
}

pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_task_matches_the_legacy_option_surface() {
        let task = SavedTask {
            source: "source:".into(),
            destination: "/tmp/target".into(),
            operation: TransferOperation::Sync,
            sync_delete_mode: Some(SyncDeleteMode::After),
            compare_mode: CompareMode::ChecksumIgnoreSize,
            shared_with_me: true,
            excludes: vec!["*.tmp".into()],
            ..Default::default()
        };
        let arguments = task.arguments(true);
        assert_eq!(arguments[0], "sync");
        assert!(arguments.contains(&"--dry-run".into()));
        assert!(arguments.contains(&"--delete-after".into()));
        assert!(arguments.contains(&"--checksum".into()));
        assert_eq!(arguments[arguments.len() - 2], "source:");
        assert_eq!(arguments[arguments.len() - 1], "/tmp/target");
    }
}
