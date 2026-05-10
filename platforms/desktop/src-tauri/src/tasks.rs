use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::models::{CompareMode, SavedTask, SyncDeleteMode, TransferDirection, TransferOperation};

#[derive(Clone)]
pub struct TaskStore {
    path: PathBuf,
    tasks: Arc<RwLock<Vec<SavedTask>>>,
}

impl TaskStore {
    pub fn open(data_dir: &Path, legacy_paths: &[PathBuf]) -> Result<Self, String> {
        fs::create_dir_all(data_dir)
            .map_err(|error| format!("Could not create the task directory: {error}"))?;
        let path = data_dir.join("tasks.json");
        let tasks = if path.exists() {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("Could not read saved tasks: {error}"))?;
            serde_json::from_str(&content)
                .map_err(|error| format!("Saved tasks are not valid JSON: {error}"))?
        } else {
            legacy_paths
                .iter()
                .find(|candidate| candidate.is_file())
                .and_then(|legacy| fs::read(legacy).ok())
                .and_then(|bytes| decode_legacy_tasks(&bytes).ok())
                .unwrap_or_default()
        };
        let store = Self {
            path,
            tasks: Arc::new(RwLock::new(tasks)),
        };
        if !store.path.exists() && !store.list().is_empty() {
            store.persist()?;
        }
        Ok(store)
    }

    pub fn list(&self) -> Vec<SavedTask> {
        self.tasks
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn save(&self, mut task: SavedTask) -> Result<SavedTask, String> {
        validate(&task)?;
        if task.id.trim().is_empty() {
            task.id = uuid::Uuid::new_v4().to_string();
        }
        {
            let mut tasks = self
                .tasks
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(existing) = tasks.iter_mut().find(|item| item.id == task.id) {
                *existing = task.clone();
            } else {
                tasks.push(task.clone());
            }
        }
        self.persist()?;
        Ok(task)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let removed = {
            let mut tasks = self
                .tasks
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let before = tasks.len();
            tasks.retain(|task| task.id != id);
            before != tasks.len()
        };
        if !removed {
            return Err("The saved task no longer exists.".into());
        }
        self.persist()
    }

    pub fn get(&self, id: &str) -> Option<SavedTask> {
        self.tasks
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .find(|task| task.id == id)
            .cloned()
    }

    fn persist(&self) -> Result<(), String> {
        let tasks = self.list();
        let json = serde_json::to_vec_pretty(&tasks)
            .map_err(|error| format!("Could not serialize saved tasks: {error}"))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)
            .map_err(|error| format!("Could not write saved tasks: {error}"))?;
        replace_file(&temporary, &self.path)
    }
}

fn replace_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    if let Err(first_error) = fs::rename(temporary, destination) {
        if !destination.exists() {
            return Err(format!("Could not commit saved tasks: {first_error}"));
        }
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace saved tasks: {error}"))?;
        fs::rename(temporary, destination)
            .map_err(|error| format!("Could not commit saved tasks: {error}"))?;
    }
    Ok(())
}

fn validate(task: &SavedTask) -> Result<(), String> {
    if task.description.trim().is_empty() {
        return Err("A task description is required.".into());
    }
    if task.source.trim().is_empty() || task.destination.trim().is_empty() {
        return Err("Both source and destination are required.".into());
    }
    if task.transfers == 0 || task.checkers == 0 {
        return Err("Transfers and checkers must be at least 1.".into());
    }
    Ok(())
}

pub fn legacy_task_paths(data_dir: &Path) -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut paths = vec![data_dir.join("tasks.bin")];
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        paths
            .push(home.join("Library/Application Support/rclone-browser/rclone-browser/tasks.bin"));
        #[cfg(target_os = "linux")]
        paths.push(home.join(".local/share/rclone-browser/rclone-browser/tasks.bin"));
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        paths.push(PathBuf::from(appdata).join("rclone-browser/rclone-browser/tasks.bin"));
    }
    paths
}

fn decode_legacy_tasks(bytes: &[u8]) -> Result<Vec<SavedTask>, String> {
    let mut reader = QtReader::new(bytes);
    let mut tasks = Vec::new();
    while !reader.is_empty() {
        let class_name = reader.string()?;
        if class_name != "JobOptions" {
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
        let bytes: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(u32::from_be_bytes(bytes))
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
        let bytes = self.take(byte_length as usize)?;
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
        String::from_utf16(&units.collect::<Vec<_>>())
            .map_err(|_| "Legacy task contains invalid text.".into())
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

    fn qstring(value: &str) -> Vec<u8> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let mut bytes = ((units.len() * 2) as u32).to_be_bytes().to_vec();
        for unit in units {
            bytes.extend(unit.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn reads_qt_strings() {
        let bytes = qstring("Rclone ☁");
        assert_eq!(QtReader::new(&bytes).string().unwrap(), "Rclone ☁");
    }

    #[test]
    fn task_store_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let store = TaskStore::open(directory.path(), &[]).unwrap();
        let saved = store
            .save(SavedTask {
                description: "Backup".into(),
                source: "/tmp/source".into(),
                destination: "remote:backup".into(),
                ..Default::default()
            })
            .unwrap();
        let reopened = TaskStore::open(directory.path(), &[]).unwrap();
        assert_eq!(reopened.get(&saved.id).unwrap().description, "Backup");
        reopened.delete(&saved.id).unwrap();
        assert!(reopened.list().is_empty());
    }
}
