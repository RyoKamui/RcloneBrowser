use std::process::Stdio;

use serde::Deserialize;
use tokio::process::Command;

use crate::models::{
    ConfigProvider, ConfigQuestion, DirectorySummary, Entry, ExportFormat, ExportOptions,
    RcloneStatus, RcloneUpdateInfo, Remote, Settings,
};

#[derive(Clone, Default)]
pub struct RcloneClient;

pub struct ExportRequest<'a> {
    pub remote: &'a str,
    pub path: &'a str,
    pub shared_with_me: bool,
    pub destination: &'a std::path::Path,
    pub format: ExportFormat,
    pub options: &'a ExportOptions,
}

impl RcloneClient {
    pub async fn status(&self, settings: &Settings, password: Option<&str>) -> RcloneStatus {
        match self.output(settings, password, ["version"]).await {
            Ok(output) => RcloneStatus {
                available: true,
                version: output.lines().next().map(str::to_owned),
                error: None,
            },
            Err(error) => RcloneStatus {
                available: false,
                version: None,
                error: Some(error),
            },
        }
    }

    pub async fn list_remotes(
        &self,
        settings: &Settings,
        password: Option<&str>,
    ) -> Result<Vec<Remote>, String> {
        let output = self
            .output(settings, password, ["listremotes", "--long", "--json"])
            .await?;
        let mut remotes: Vec<Remote> = serde_json::from_str(&output)
            .map_err(|error| format!("rclone returned invalid remote data: {error}"))?;
        for remote in &mut remotes {
            remote.display_name = remote.name.clone();
        }
        remotes.push(Remote {
            name: "__local__".into(),
            remote_type: "local".into(),
            description: "Files on this computer".into(),
            is_local: true,
            display_name: "Local filesystem".into(),
        });
        Ok(remotes)
    }

    pub async fn config_providers(
        &self,
        settings: &Settings,
        password: Option<&str>,
    ) -> Result<Vec<ConfigProvider>, String> {
        let output = self
            .output(settings, password, ["config", "providers"])
            .await?;
        let mut providers: Vec<ConfigProvider> = serde_json::from_str(&output)
            .map_err(|error| format!("rclone returned invalid provider data: {error}"))?;
        providers.retain(|provider| !provider.hide && !provider.name.trim().is_empty());
        providers.sort_by(|left, right| {
            left.description
                .to_lowercase()
                .cmp(&right.description.to_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(providers)
    }

    pub async fn start_config_create(
        &self,
        settings: &Settings,
        password: Option<&str>,
        name: &str,
        provider: &str,
    ) -> Result<ConfigQuestion, String> {
        validate_config_identity(name, provider)?;
        let remotes = self.list_remotes(settings, password).await?;
        if remotes.iter().any(|remote| remote.name == name.trim()) {
            return Err(format!(
                "A location named '{}' already exists.",
                name.trim()
            ));
        }
        self.config_question(
            settings,
            password,
            vec![
                "config".into(),
                "create".into(),
                name.trim().into(),
                provider.trim().into(),
                "--all".into(),
                "--non-interactive".into(),
            ],
        )
        .await
    }

    pub async fn continue_config_create(
        &self,
        settings: &Settings,
        password: Option<&str>,
        name: &str,
        provider: &str,
        state: &str,
        result: &str,
    ) -> Result<ConfigQuestion, String> {
        validate_config_identity(name, provider)?;
        if state.trim().is_empty() {
            return Err("The rclone configuration session has already finished.".into());
        }
        self.config_question(
            settings,
            password,
            vec![
                "config".into(),
                "create".into(),
                name.trim().into(),
                provider.trim().into(),
                "--all".into(),
                "--non-interactive".into(),
                "--continue".into(),
                "--state".into(),
                state.into(),
                "--result".into(),
                result.into(),
            ],
        )
        .await
    }

    pub async fn delete_config_remote(
        &self,
        settings: &Settings,
        password: Option<&str>,
        name: &str,
    ) -> Result<(), String> {
        if name.trim().is_empty() || name.contains([':', '/', '\\', '\n', '\r']) {
            return Err("The pending location name is invalid.".into());
        }
        self.output(settings, password, ["config", "delete", name.trim()])
            .await
            .map(|_| ())
    }

    async fn config_question(
        &self,
        settings: &Settings,
        password: Option<&str>,
        arguments: Vec<String>,
    ) -> Result<ConfigQuestion, String> {
        let output = self.output(settings, password, arguments).await?;
        let question: ConfigQuestion = serde_json::from_str(&output).map_err(|error| {
            format!("rclone returned an invalid configuration question: {error}")
        })?;
        if !question.error.trim().is_empty() {
            return Err(question.error.clone());
        }
        Ok(question)
    }

    pub async fn list_entries(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
        shared_with_me: bool,
    ) -> Result<Vec<Entry>, String> {
        let target = browser_target(remote, path);
        let mut arguments = vec!["lsjson".to_owned(), target];
        if shared_with_me {
            arguments.push("--drive-shared-with-me".into());
        }
        if !settings.show_hidden {
            arguments.extend([
                "--exclude".into(),
                ".*/**".into(),
                "--exclude".into(),
                ".*".into(),
            ]);
        }
        let output = self.output(settings, password, arguments).await?;
        let raw: Vec<RawEntry> = serde_json::from_str(&output)
            .map_err(|error| format!("rclone returned an invalid file listing: {error}"))?;
        let mut entries: Vec<Entry> = raw
            .into_iter()
            .map(|entry| Entry {
                path: browser_join(remote, path, &entry.name),
                name: entry.name,
                is_dir: entry.is_dir,
                size: (!entry.is_dir).then_some(entry.size.max(0) as u64),
                mod_time: non_empty(entry.mod_time),
                mime_type: non_empty(entry.mime_type),
            })
            .collect();
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub async fn create_folder(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
    ) -> Result<(), String> {
        self.output(
            settings,
            password,
            ["mkdir".into(), browser_target(remote, path)],
        )
        .await
        .map(|_| ())
    }

    pub async fn rename_entry(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
        new_name: &str,
    ) -> Result<(), String> {
        if new_name.trim().is_empty() || new_name.contains(['/', '\\']) {
            return Err("The new name must be a single non-empty path component.".into());
        }
        let destination = if remote == "__local__" {
            std::path::Path::new(path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(std::path::MAIN_SEPARATOR_STR))
                .join(new_name.trim())
                .to_string_lossy()
                .into_owned()
        } else {
            join_path(&parent_path(path), new_name.trim())
        };
        self.output(
            settings,
            password,
            [
                "moveto".into(),
                browser_target(remote, path),
                browser_target(remote, &destination),
            ],
        )
        .await
        .map(|_| ())
    }

    pub async fn delete_entry(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
        is_dir: bool,
    ) -> Result<(), String> {
        let operation = if is_dir { "purge" } else { "deletefile" };
        self.output(
            settings,
            password,
            [operation.into(), browser_target(remote, path)],
        )
        .await
        .map(|_| ())
    }

    pub async fn public_link(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
        shared_with_me: bool,
    ) -> Result<String, String> {
        let mut arguments = vec!["link".into(), browser_target(remote, path)];
        if shared_with_me {
            arguments.push("--drive-shared-with-me".into());
        }
        self.output(settings, password, arguments)
            .await
            .map(|output| output.trim().to_owned())
    }

    pub async fn move_entry(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        source: &str,
        destination: &str,
    ) -> Result<(), String> {
        self.output(
            settings,
            password,
            [
                "moveto".into(),
                browser_target(remote, source),
                browser_target(remote, destination),
            ],
        )
        .await
        .map(|_| ())
    }

    pub async fn directory_size(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
        shared_with_me: bool,
    ) -> Result<DirectorySummary, String> {
        let mut arguments = vec!["size".into(), browser_target(remote, path), "--json".into()];
        if shared_with_me {
            arguments.push("--drive-shared-with-me".into());
        }
        let output = self.output(settings, password, arguments).await?;
        serde_json::from_str(&output)
            .map_err(|error| format!("rclone returned invalid size information: {error}"))
    }

    pub async fn directory_tree(
        &self,
        settings: &Settings,
        password: Option<&str>,
        remote: &str,
        path: &str,
        shared_with_me: bool,
    ) -> Result<String, String> {
        let mut arguments = vec!["tree".into(), "-d".into(), browser_target(remote, path)];
        if shared_with_me {
            arguments.push("--drive-shared-with-me".into());
        }
        self.output(settings, password, arguments).await
    }

    pub async fn export_listing(
        &self,
        settings: &Settings,
        password: Option<&str>,
        request: ExportRequest<'_>,
    ) -> Result<u64, String> {
        let ExportRequest {
            remote,
            path,
            shared_with_me,
            destination,
            format,
            options,
        } = request;
        let arguments = export_arguments(remote, path, shared_with_me, options);
        let output = self.output(settings, password, arguments).await?;
        let entries: Vec<RawEntry> = serde_json::from_str(&output)
            .map_err(|error| format!("rclone returned an invalid export listing: {error}"))?;
        let mut result = String::new();
        for entry in &entries {
            let name = if entry.path.is_empty() {
                &entry.name
            } else {
                &entry.path
            };
            match format {
                ExportFormat::Txt => {
                    result.push_str(name);
                    result.push('\n');
                }
                ExportFormat::Csv => {
                    result.push_str(&csv_field(name));
                    result.push(',');
                    result.push_str(&csv_field(&entry.mod_time));
                    result.push(',');
                    result.push_str(&entry.size.max(0).to_string());
                    result.push('\n');
                }
            }
        }
        std::fs::write(destination, result)
            .map_err(|error| format!("Could not write '{}': {error}", destination.display()))?;
        Ok(entries.len() as u64)
    }

    pub async fn check_rclone_update(
        &self,
        settings: &Settings,
        password: Option<&str>,
    ) -> Result<RcloneUpdateInfo, String> {
        let output = self
            .output(settings, password, ["selfupdate", "--check"])
            .await?;
        rclone_browser_shared::parse_rclone_update(&output)
    }

    pub fn command(
        &self,
        settings: &Settings,
        password: Option<&str>,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    ) -> Command {
        let rclone_path = resolved_path(settings, &settings.rclone_path);
        let mut command = Command::new(rclone_path);
        if let Some(config_path) = settings
            .config_path
            .as_deref()
            .filter(|path| !path.is_empty())
        {
            command
                .arg("--config")
                .arg(resolved_path(settings, config_path));
        }
        command.args(&settings.advanced_args);
        command.args(arguments);
        if let Some(password) = password.filter(|password| !password.is_empty()) {
            command.env("RCLONE_CONFIG_PASS", password);
        }
        if settings.use_proxy {
            set_env_if_present(&mut command, "HTTP_PROXY", &settings.http_proxy);
            set_env_if_present(&mut command, "HTTPS_PROXY", &settings.https_proxy);
            set_env_if_present(&mut command, "NO_PROXY", &settings.no_proxy);
        }
        command
    }

    async fn output(
        &self,
        settings: &Settings,
        password: Option<&str>,
        arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    ) -> Result<String, String> {
        let output = self
            .command(settings, password, arguments)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|error| format!("Could not start '{}': {error}", settings.rclone_path))?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(if message.is_empty() {
                format!("rclone exited with {}", output.status)
            } else {
                message
            });
        }
        String::from_utf8(output.stdout)
            .map_err(|error| format!("rclone output was not valid UTF-8: {error}"))
    }
}

fn validate_config_identity(name: &str, provider: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name for the new location.".into());
    }
    if name.contains([':', '/', '\\', '\n', '\r', '[', ']']) {
        return Err(
            "Location names cannot contain colons, slashes, brackets, or new lines.".into(),
        );
    }
    if provider.trim().is_empty()
        || provider
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
    {
        return Err("Choose a valid rclone storage provider.".into());
    }
    Ok(())
}

fn push_argument(arguments: &mut Vec<String>, name: &str, value: &str) {
    if !value.trim().is_empty() {
        arguments.extend([name.to_owned(), value.trim().to_owned()]);
    }
}

fn export_arguments(
    remote: &str,
    path: &str,
    shared_with_me: bool,
    options: &ExportOptions,
) -> Vec<String> {
    let mut arguments = vec![
        "lsjson".into(),
        browser_target(remote, path),
        "--recursive".into(),
        "--files-only".into(),
    ];
    if shared_with_me {
        arguments.push("--drive-shared-with-me".into());
    }
    if options.one_file_system {
        arguments.push("--one-file-system".into());
    }
    push_argument(&mut arguments, "--min-size", &options.min_size);
    push_argument(&mut arguments, "--min-age", &options.min_age);
    push_argument(&mut arguments, "--max-age", &options.max_age);
    if options.max_depth > 0 {
        arguments.extend(["--max-depth".into(), options.max_depth.to_string()]);
    }
    for exclude in options
        .excludes
        .iter()
        .filter(|value| !value.trim().is_empty())
    {
        arguments.extend(["--exclude".into(), exclude.trim().to_owned()]);
    }
    arguments.extend(options.extra_args.clone());
    arguments
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawEntry {
    #[serde(default)]
    path: String,
    name: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    is_dir: bool,
    #[serde(default)]
    mod_time: String,
    #[serde(default)]
    mime_type: String,
}

pub fn remote_target(remote: &str, path: &str) -> String {
    format!(
        "{}:{}",
        remote.trim().trim_end_matches(':'),
        path.trim().trim_matches('/')
    )
}

pub fn browser_target(remote: &str, path: &str) -> String {
    if remote == "__local__" {
        if path.trim().is_empty() {
            std::path::MAIN_SEPARATOR.to_string()
        } else {
            path.to_owned()
        }
    } else {
        remote_target(remote, path)
    }
}

pub fn browser_join(remote: &str, parent: &str, name: &str) -> String {
    if remote == "__local__" {
        let base = if parent.is_empty() {
            std::path::Path::new(std::path::MAIN_SEPARATOR_STR)
        } else {
            std::path::Path::new(parent)
        };
        base.join(name).to_string_lossy().into_owned()
    } else {
        join_path(parent, name)
    }
}

pub fn join_path(parent: &str, name: &str) -> String {
    let parent = parent.trim_matches('/');
    let name = name.trim_matches('/');
    if parent.is_empty() {
        name.to_owned()
    } else if name.is_empty() {
        parent.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

pub fn parent_path(path: &str) -> String {
    path.trim_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_owned())
        .unwrap_or_default()
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn set_env_if_present(command: &mut Command, key: &str, value: &str) {
    if !value.trim().is_empty() {
        command
            .env(key, value.trim())
            .env(key.to_lowercase(), value.trim());
    }
}

pub fn resolved_path(settings: &Settings, value: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(value);
    if path.is_relative()
        && value != "rclone"
        && let Some(base) = &settings.portable_base
    {
        return base.join(path);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_remote_targets_without_double_colons() {
        assert_eq!(remote_target("photos:", "/2025/trip/"), "photos:2025/trip");
        assert_eq!(remote_target("photos", ""), "photos:");
        assert_eq!(
            browser_target("__local__", ""),
            std::path::MAIN_SEPARATOR.to_string()
        );
        assert_eq!(
            browser_join("__local__", "", "Users"),
            format!("{}Users", std::path::MAIN_SEPARATOR)
        );
    }

    #[test]
    fn joins_and_finds_parent_paths() {
        assert_eq!(join_path("", "Pictures"), "Pictures");
        assert_eq!(join_path("Pictures", "Trip"), "Pictures/Trip");
        assert_eq!(parent_path("Pictures/Trip"), "Pictures");
        assert_eq!(parent_path("Pictures"), "");
    }

    #[test]
    fn deserializes_rclone_lsjson() {
        let json = r#"[{"Path":"Pictures","Name":"Pictures","Size":-1,"MimeType":"inode/directory","ModTime":"2026-01-01T00:00:00Z","IsDir":true},{"Path":"photo.jpg","Name":"photo.jpg","Size":42,"MimeType":"image/jpeg","ModTime":"2026-01-02T03:04:05Z","IsDir":false}]"#;
        let raw: Vec<RawEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(raw[0].name, "Pictures");
        assert_eq!(raw[0].size, -1);
        assert!(raw[0].is_dir);
        assert_eq!(raw[1].name, "photo.jpg");
        assert_eq!(raw[1].size, 42);
        assert!(!raw[1].is_dir);
    }

    #[test]
    fn quotes_csv_values() {
        assert_eq!(csv_field("a,\"b\""), "\"a,\"\"b\"\"\"");
    }

    #[test]
    fn preserves_the_legacy_export_option_surface() {
        let arguments = export_arguments(
            "drive",
            "Archive",
            true,
            &ExportOptions {
                one_file_system: true,
                min_size: "10M".into(),
                min_age: "1h".into(),
                max_age: "30d".into(),
                max_depth: 4,
                excludes: vec!["*.tmp".into()],
                extra_args: vec!["--fast-list".into()],
            },
        );
        for expected in [
            "--drive-shared-with-me",
            "--one-file-system",
            "--min-size",
            "--min-age",
            "--max-age",
            "--max-depth",
            "--exclude",
            "--fast-list",
        ] {
            assert!(arguments.iter().any(|argument| argument == expected));
        }
    }

    #[cfg(unix)]
    #[test]
    fn exercises_the_rclone_feature_surface_against_a_cli_fixture() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/fake-rclone");
        let mut settings = Settings {
            rclone_path: fixture.to_string_lossy().into_owned(),
            ..Default::default()
        };
        settings.show_hidden = true;
        let client = RcloneClient;
        tauri::async_runtime::block_on(async {
            let status = client.status(&settings, None).await;
            assert!(status.available);
            assert_eq!(status.version.as_deref(), Some("rclone v1.99.0-fixture"));

            let remotes = client.list_remotes(&settings, None).await.unwrap();
            assert_eq!(remotes[0].name, "fixture");
            assert!(remotes.iter().any(|remote| remote.is_local));

            let providers = client.config_providers(&settings, None).await.unwrap();
            assert_eq!(providers.len(), 1);
            assert_eq!(providers[0].name, "fixture");
            let question = client
                .start_config_create(&settings, None, "new-fixture", "fixture")
                .await
                .unwrap();
            assert_eq!(question.option.as_ref().unwrap().name, "endpoint");
            let complete = client
                .continue_config_create(
                    &settings,
                    None,
                    "new-fixture",
                    "fixture",
                    &question.state,
                    "https://example.invalid",
                )
                .await
                .unwrap();
            assert!(complete.state.is_empty());
            assert!(complete.option.is_none());
            client
                .delete_config_remote(&settings, None, "new-fixture")
                .await
                .unwrap();

            let entries = client
                .list_entries(&settings, None, "fixture", "", true)
                .await
                .unwrap();
            assert!(entries[0].is_dir);
            assert_eq!(entries[0].size, None);
            assert_eq!(entries[1].size, Some(12));

            client
                .create_folder(&settings, None, "fixture", "Created")
                .await
                .unwrap();
            client
                .rename_entry(&settings, None, "fixture", "Created", "Renamed")
                .await
                .unwrap();
            client
                .move_entry(&settings, None, "fixture", "Renamed", "Docs/Renamed")
                .await
                .unwrap();
            client
                .delete_entry(&settings, None, "fixture", "Docs/Renamed", true)
                .await
                .unwrap();
            assert_eq!(
                client
                    .public_link(&settings, None, "fixture", "readme.txt", true)
                    .await
                    .unwrap(),
                "https://example.invalid/public-link"
            );
            assert_eq!(
                client
                    .directory_size(&settings, None, "fixture", "", true)
                    .await
                    .unwrap()
                    .bytes,
                75
            );
            assert!(
                client
                    .directory_tree(&settings, None, "fixture", "", true)
                    .await
                    .unwrap()
                    .contains("Archive")
            );

            let directory = tempfile::tempdir().unwrap();
            let destination = directory.path().join("listing.csv");
            let export_options = ExportOptions {
                one_file_system: true,
                min_size: "10B".into(),
                max_depth: 4,
                excludes: vec!["*.tmp".into()],
                ..Default::default()
            };
            let count = client
                .export_listing(
                    &settings,
                    None,
                    ExportRequest {
                        remote: "fixture",
                        path: "",
                        shared_with_me: true,
                        destination: &destination,
                        format: ExportFormat::Csv,
                        options: &export_options,
                    },
                )
                .await
                .unwrap();
            assert_eq!(count, 2);
            assert!(
                std::fs::read_to_string(destination)
                    .unwrap()
                    .contains("Docs/notes.txt")
            );
            let update = client.check_rclone_update(&settings, None).await.unwrap();
            assert_eq!(
                update
                    .stable
                    .as_ref()
                    .map(|release| release.version.as_str()),
                Some("1.99.0-fixture")
            );
            assert!(!update.stable_update_available);
        });
    }
}
