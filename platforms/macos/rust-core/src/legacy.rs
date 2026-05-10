use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::models::{
    CompareMode, IconSize, SavedTask, Settings, SyncDeleteMode, Theme, TransferDirection,
    TransferOperation,
};

pub fn import_settings(data_dir: &Path) -> Option<Settings> {
    legacy_settings_paths(data_dir)
        .into_iter()
        .find_map(|path| import_settings_file(&path))
}

pub fn import_tasks(data_dir: &Path) -> Option<Vec<SavedTask>> {
    legacy_task_paths(data_dir)
        .into_iter()
        .find(|path| path.is_file())
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| decode_tasks(&bytes).ok())
}

fn legacy_settings_paths(data_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        data_dir.join("Rclone Browser.ini"),
        data_dir.join("rclone-browser.ini"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join("Library/Preferences/com.rclone-browser.rclone-browser.plist"));
        paths.push(
            home.join(
                "Library/Application Support/rclone-browser/rclone-browser/Rclone Browser.ini",
            ),
        );
    }
    paths.extend(portable_paths(["Rclone Browser.ini", "rclone-browser.ini"]));
    paths
}

fn legacy_task_paths(data_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![data_dir.join("tasks.bin")];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths
            .push(home.join("Library/Application Support/rclone-browser/rclone-browser/tasks.bin"));
    }
    paths.extend(portable_paths(["tasks.bin"]));
    paths
}

fn portable_paths<const N: usize>(names: [&str; N]) -> Vec<PathBuf> {
    let Some(executable) = std::env::current_exe().ok() else {
        return Vec::new();
    };
    let bundle_parent = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent);
    bundle_parent
        .map(|parent| names.into_iter().map(|name| parent.join(name)).collect())
        .unwrap_or_default()
}

fn import_settings_file(path: &Path) -> Option<Settings> {
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
    settings.notify_finished_transfers = boolean(&values, "notifyFinishedTransfers", true);
    settings.close_to_tray = boolean(&values, "closeToTray", false);
    settings.always_show_tray = boolean(&values, "alwaysShowInTray", false);
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
            let name = match section.as_str() {
                "Settings" => key.trim().to_owned(),
                "Export" => format!("Export/{}", key.trim()),
                _ => continue,
            };
            values.insert(name, unescape_ini(value.trim()));
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

fn decode_tasks(bytes: &[u8]) -> Result<Vec<SavedTask>, String> {
    let mut reader = QtReader::new(bytes);
    let mut tasks = Vec::new();
    while !reader.is_empty() {
        if reader.string()? != "JobOptions" {
            return Err("Legacy task file contains an unknown record.".into());
        }
        let version = reader.i32()?;
        if !(1..=3).contains(&version) {
            return Err(format!("Legacy task version {version} is not supported."));
        }
        let description = reader.string()?;
        let direction = match reader.u32()? {
            1 => TransferDirection::Upload,
            2 => TransferDirection::Download,
            _ => TransferDirection::Copy,
        };
        let operation = match reader.u32()? {
            2 => TransferOperation::Move,
            3 => TransferOperation::Sync,
            _ => TransferOperation::Copy,
        };
        let sync = reader.boolean()?;
        let timing = match reader.u32()? {
            0 => Some(SyncDeleteMode::During),
            1 => Some(SyncDeleteMode::After),
            2 => Some(SyncDeleteMode::Before),
            _ => None,
        };
        let update = reader.boolean()?;
        let ignore_existing = reader.boolean()?;
        let compare = reader.boolean()?;
        let compare_value = reader.u32()?;
        let _verbose = reader.boolean()?;
        let one_file_system = reader.boolean()?;
        let no_update_modtime = reader.boolean()?;
        let transfers = parse_number(&reader.string()?, 4);
        let checkers = parse_number(&reader.string()?, 8);
        let bandwidth = reader.string()?;
        let min_size = reader.string()?;
        let min_age = reader.string()?;
        let max_age = reader.string()?;
        let max_depth = reader.i32()?.max(0) as u32;
        let connect_timeout_seconds = parse_number(&reader.string()?, 60) as u32;
        let idle_timeout_seconds = parse_number(&reader.string()?, 300) as u32;
        let retries = parse_number(&reader.string()?, 3);
        let low_level_retries = parse_number(&reader.string()?, 10);
        let delete_excluded = reader.boolean()?;
        let excludes = reader
            .string()?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        let extra_args = reader
            .string()?
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let shared_with_me = reader.boolean()?;
        let source = reader.string()?;
        let destination = reader.string()?;
        let is_directory = if version >= 2 {
            reader.boolean()?
        } else {
            false
        };
        let id = if version >= 3 {
            reader
                .uuid()?
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        tasks.push(SavedTask {
            id,
            description,
            direction,
            operation,
            source,
            destination,
            is_directory,
            sync_delete_mode: sync.then_some(timing).flatten(),
            update,
            ignore_existing,
            compare_mode: if compare {
                match compare_value {
                    1 => CompareMode::Checksum,
                    2 => CompareMode::IgnoreSize,
                    3 => CompareMode::SizeOnly,
                    4 => CompareMode::ChecksumIgnoreSize,
                    _ => CompareMode::SizeAndModTime,
                }
            } else {
                CompareMode::SizeAndModTime
            },
            one_file_system,
            no_update_modtime,
            transfers,
            checkers,
            bandwidth,
            min_size,
            min_age,
            max_age,
            max_depth,
            connect_timeout_seconds,
            idle_timeout_seconds,
            retries,
            low_level_retries,
            delete_excluded,
            excludes,
            extra_args,
            shared_with_me,
        });
    }
    Ok(tasks)
}

fn parse_number<T>(value: &str, fallback: T) -> T
where
    T: std::str::FromStr,
{
    value.parse().unwrap_or(fallback)
}

struct QtReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> QtReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "Legacy task file ended unexpectedly.".to_owned())?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn boolean(&mut self) -> Result<bool, String> {
        Ok(self.take(1)?[0] != 0)
    }

    fn string(&mut self) -> Result<String, String> {
        let byte_length = self.u32()?;
        if byte_length == u32::MAX {
            return Ok(String::new());
        }
        if byte_length % 2 != 0 {
            return Err("Legacy task contains an invalid UTF-16 string.".into());
        }
        let units = self
            .take(byte_length as usize)?
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|_| "Legacy task contains invalid text.".into())
    }

    fn uuid(&mut self) -> Result<Option<String>, String> {
        let bytes = self.take(16)?;
        if bytes.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        let data1 = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let data2 = u16::from_be_bytes(bytes[4..6].try_into().unwrap());
        let data3 = u16::from_be_bytes(bytes[6..8].try_into().unwrap());
        Ok(Some(format!(
            "{data1:08x}-{data2:04x}-{data3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_qsettings_ini() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Rclone Browser.ini");
        fs::write(&path, "[Settings]\nrclone=tools/rclone\nshowHidden=false\nrowColors=false\niconSize=large\n[Export]\nspinMaxDepth=3\n").unwrap();
        let settings = import_settings_file(&path).unwrap();
        assert_eq!(settings.rclone_path, "tools/rclone");
        assert!(!settings.show_hidden);
        assert!(!settings.alternating_rows);
        assert!(matches!(settings.icon_size, IconSize::Large));
        assert_eq!(settings.export_options.max_depth, 3);
    }

    #[test]
    fn reads_qt_strings() {
        let units: Vec<u16> = "Rclone ☁".encode_utf16().collect();
        let mut bytes = ((units.len() * 2) as u32).to_be_bytes().to_vec();
        for unit in units {
            bytes.extend(unit.to_be_bytes());
        }
        assert_eq!(QtReader::new(&bytes).string().unwrap(), "Rclone ☁");
    }
}
