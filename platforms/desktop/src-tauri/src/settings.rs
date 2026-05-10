use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::models::{IconSize, Settings, Theme};

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
    current: Arc<RwLock<Settings>>,
}

impl SettingsStore {
    pub fn open(
        config_dir: &Path,
        portable: bool,
        legacy_paths: &[PathBuf],
    ) -> Result<Self, String> {
        fs::create_dir_all(config_dir)
            .map_err(|error| format!("Could not create the settings directory: {error}"))?;
        let path = config_dir.join("settings.json");
        let mut current = if path.exists() {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("Could not read settings: {error}"))?;
            serde_json::from_str(&content)
                .map_err(|error| format!("Settings are not valid JSON: {error}"))?
        } else {
            legacy_paths
                .iter()
                .find_map(|legacy| import_legacy(legacy))
                .unwrap_or_default()
        };
        if portable {
            current.portable_base = Some(config_dir.to_path_buf());
        }
        let store = Self {
            path,
            current: Arc::new(RwLock::new(current)),
        };
        if !store.path.exists() && legacy_paths.iter().any(|path| path.exists()) {
            store.persist()?;
        }
        Ok(store)
    }

    pub fn get(&self) -> Settings {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn save(&self, mut settings: Settings) -> Result<(), String> {
        validate(&settings)?;
        settings.portable_base = self
            .current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .portable_base
            .clone();
        *self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&self.get())
            .map_err(|error| format!("Could not serialize settings: {error}"))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)
            .map_err(|error| format!("Could not write settings: {error}"))?;
        if let Err(first_error) = fs::rename(&temporary, &self.path) {
            if !self.path.exists() {
                return Err(format!("Could not commit settings: {first_error}"));
            }
            fs::remove_file(&self.path)
                .map_err(|error| format!("Could not replace settings: {error}"))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| format!("Could not commit settings: {error}"))?;
        }
        Ok(())
    }
}

pub fn legacy_settings_paths(data_dir: &Path) -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut paths = vec![
        data_dir.join("Rclone Browser.ini"),
        data_dir.join("rclone-browser.ini"),
        data_dir.join("rclone-browser.conf"),
    ];
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        paths.push(home.join("Library/Preferences/com.rclone-browser.rclone-browser.plist"));
        #[cfg(target_os = "linux")]
        paths.push(home.join(".config/rclone-browser/rclone-browser.conf"));
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        paths.push(PathBuf::from(appdata).join("rclone-browser/rclone-browser/rclone-browser.ini"));
    }
    paths
}

fn import_legacy(path: &Path) -> Option<Settings> {
    if !path.is_file() {
        return None;
    }
    let values = if path.extension().and_then(|value| value.to_str()) == Some("plist") {
        plist_values(path)?
    } else {
        ini_values(path)?
    };
    let mut settings = Settings::default();
    assign_string(&values, "rclone", &mut settings.rclone_path);
    settings.config_path = optional_string(&values, "rcloneConf");
    settings.default_download_dir = optional_string(&values, "defaultDownloadDir");
    settings.default_upload_dir = optional_string(&values, "defaultUploadDir");
    settings.default_download_args = split_arguments(&values, "defaultDownloadOptions");
    settings.default_upload_args = split_arguments(&values, "defaultUploadOptions");
    settings.advanced_args = split_arguments(&values, "defaultRcloneOptions");
    assign_string(&values, "stream", &mut settings.stream_command);
    let mount = split_arguments(&values, "mount");
    if !mount.is_empty() {
        settings.mount_args = mount;
    }
    settings.show_hidden = boolean(&values, "showHidden", settings.show_hidden);
    settings.show_folder_icons = boolean(&values, "showFolderIcons", true);
    settings.show_file_icons = boolean(&values, "showFileIcons", true);
    settings.alternating_rows = boolean(&values, "rowColors", true);
    settings.icon_size = match values.get("iconSize").map(String::as_str) {
        Some("small") => IconSize::Small,
        Some("large") => IconSize::Large,
        _ => IconSize::Medium,
    };
    settings.close_to_tray = boolean(&values, "closeToTray", false);
    settings.always_show_tray = boolean(&values, "alwaysShowInTray", false);
    settings.notify_finished_transfers = boolean(&values, "notifyFinishedTransfers", true);
    settings.check_app_updates = boolean(&values, "checkRcloneBrowserUpdates", true);
    settings.check_rclone_updates = boolean(&values, "checkRcloneUpdates", true);
    settings.use_proxy = boolean(&values, "useProxy", false);
    assign_string(&values, "http_proxy", &mut settings.http_proxy);
    assign_string(&values, "https_proxy", &mut settings.https_proxy);
    assign_string(&values, "no_proxy", &mut settings.no_proxy);
    if boolean(&values, "darkMode", false) {
        settings.theme = Theme::Dark;
    }
    settings.export_options.one_file_system = boolean(&values, "Export/checkSameFilesystem", false);
    assign_string(
        &values,
        "Export/textMinSize",
        &mut settings.export_options.min_size,
    );
    assign_string(
        &values,
        "Export/textMinAge",
        &mut settings.export_options.min_age,
    );
    assign_string(
        &values,
        "Export/textMaxAge",
        &mut settings.export_options.max_age,
    );
    settings.export_options.max_depth = values
        .get("Export/spinMaxDepth")
        .and_then(|value| value.parse().ok())
        .unwrap_or_default();
    settings.export_options.extra_args = split_arguments(&values, "Export/textExtra");
    Some(settings)
}

fn plist_values(path: &Path) -> Option<HashMap<String, String>> {
    let dictionary = plist::Value::from_file(path).ok()?.into_dictionary()?;
    Some(
        dictionary
            .into_iter()
            .filter_map(|(key, value)| {
                let name = if let Some(name) = key.strip_prefix("Settings.") {
                    name.to_owned()
                } else {
                    format!("Export/{}", key.strip_prefix("Export.")?)
                };
                let value = match value {
                    plist::Value::String(value) => value,
                    plist::Value::Boolean(value) => value.to_string(),
                    plist::Value::Integer(value) => value.to_string(),
                    _ => return None,
                };
                Some((name, value))
            })
            .collect(),
    )
}

fn ini_values(path: &Path) -> Option<HashMap<String, String>> {
    let content = fs::read_to_string(path).ok()?;
    let mut section = String::new();
    let mut values = HashMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_owned();
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if section == "Settings" {
                values.insert(key.to_owned(), unescape_ini(value.trim()));
            } else if section == "Export" {
                values.insert(format!("Export/{key}"), unescape_ini(value.trim()));
            }
        }
    }
    (!values.is_empty()).then_some(values)
}

fn unescape_ini(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace("\\\\", "\\")
}

fn split_arguments(values: &HashMap<String, String>, key: &str) -> Vec<String> {
    values
        .get(key)
        .map(|value| value.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn optional_string(values: &HashMap<String, String>, key: &str) -> Option<String> {
    values.get(key).filter(|value| !value.is_empty()).cloned()
}

fn assign_string(values: &HashMap<String, String>, key: &str, target: &mut String) {
    if let Some(value) = values.get(key).filter(|value| !value.is_empty()) {
        target.clone_from(value);
    }
}

fn boolean(values: &HashMap<String, String>, key: &str, fallback: bool) -> bool {
    values
        .get(key)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(fallback)
}

fn validate(settings: &Settings) -> Result<(), String> {
    if settings.rclone_path.trim().is_empty() {
        return Err("The rclone executable path cannot be empty.".into());
    }
    if settings
        .advanced_args
        .iter()
        .any(|argument| argument.is_empty())
    {
        return Err("Advanced arguments cannot contain an empty value.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(directory.path(), false, &[]).unwrap();
        let mut expected = store.get();
        expected.show_hidden = true;
        expected.advanced_args = vec!["--fast-list".into()];
        store.save(expected.clone()).unwrap();

        let reopened = SettingsStore::open(directory.path(), false, &[]).unwrap();
        assert_eq!(reopened.get(), expected);
    }

    #[test]
    fn rejects_an_empty_executable() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(directory.path(), false, &[]).unwrap();
        let mut settings = store.get();
        settings.rclone_path.clear();
        assert!(store.save(settings).is_err());
    }

    #[test]
    fn imports_portable_qsettings_ini() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("rclone-browser.ini");
        fs::write(
            &legacy,
            "[Settings]\nrclone=tools/rclone\nrcloneConf=config/rclone.conf\nshowHidden=false\nshowFileIcons=false\nrowColors=false\niconSize=large\ndefaultUploadOptions=--fast-list --checksum\ncloseToTray=true\n[Export]\ncheckSameFilesystem=true\ntextMinSize=10M\nspinMaxDepth=3\n",
        )
        .unwrap();
        let store = SettingsStore::open(directory.path(), true, &[legacy]).unwrap();
        let settings = store.get();
        assert_eq!(settings.rclone_path, "tools/rclone");
        assert_eq!(settings.config_path.as_deref(), Some("config/rclone.conf"));
        assert!(!settings.show_hidden);
        assert!(settings.close_to_tray);
        assert!(!settings.show_file_icons);
        assert!(!settings.alternating_rows);
        assert_eq!(settings.icon_size, IconSize::Large);
        assert!(settings.export_options.one_file_system);
        assert_eq!(settings.export_options.min_size, "10M");
        assert_eq!(settings.export_options.max_depth, 3);
        assert_eq!(settings.default_upload_args, ["--fast-list", "--checksum"]);
        assert_eq!(settings.portable_base.as_deref(), Some(directory.path()));
    }
}
