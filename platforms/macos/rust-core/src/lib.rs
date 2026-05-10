mod legacy;
mod models;

use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard, OnceLock},
    thread,
};

use models::*;
use rclone_browser_shared::parse_rclone_update;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

const APP_VERSION: &str = "3.0.0";

struct CoreState {
    data_dir: PathBuf,
    settings: Mutex<Settings>,
    password: Mutex<Option<String>>,
    pending_configs: Mutex<HashSet<String>>,
    pending_updates: Mutex<HashSet<String>>,
    tasks: Mutex<Vec<SavedTask>>,
    transfers: Mutex<HashMap<String, TransferSnapshot>>,
    activities: Mutex<HashMap<String, ActivitySnapshot>>,
    processes: Mutex<HashMap<String, Vec<u32>>>,
}

static STATE: OnceLock<CoreState> = OnceLock::new();

fn state() -> &'static CoreState {
    STATE.get_or_init(|| {
        let data_dir = data_directory();
        let _ = fs::create_dir_all(&data_dir);
        let settings_path = data_dir.join("settings.json");
        let imported_settings = !settings_path.exists();
        let settings = load_json::<Settings>(&settings_path)
            .or_else(|| legacy_data_dir().and_then(|path| load_json(&path.join("settings.json"))))
            .or_else(|| legacy::import_settings(&data_dir))
            .unwrap_or_default();
        if imported_settings {
            let _ = persist_json(&settings_path, &settings);
        }
        let tasks_path = data_dir.join("tasks.json");
        let imported_tasks = !tasks_path.exists();
        let tasks = load_json::<Vec<SavedTask>>(&tasks_path)
            .or_else(|| legacy_data_dir().and_then(|path| load_json(&path.join("tasks.json"))))
            .or_else(|| legacy::import_tasks(&data_dir))
            .unwrap_or_default();
        if imported_tasks && !tasks.is_empty() {
            let _ = persist_json(&tasks_path, &tasks);
        }
        CoreState {
            data_dir,
            settings: Mutex::new(settings),
            password: Mutex::new(None),
            pending_configs: Mutex::new(HashSet::new()),
            pending_updates: Mutex::new(HashSet::new()),
            tasks: Mutex::new(tasks),
            transfers: Mutex::new(HashMap::new()),
            activities: Mutex::new(HashMap::new()),
            processes: Mutex::new(HashMap::new()),
        }
    })
}

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn data_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("RCLONE_BROWSER_DATA_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = portable_data_directory() {
        return path;
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/Rclone Browser")
}

fn portable_data_directory() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let bundle = executable.parent()?.parent()?.parent()?;
    let parent = bundle.parent()?;
    let bundle_name = bundle.file_stem()?.to_string_lossy();
    let markers = [
        parent.join(format!("{bundle_name}.ini")),
        parent.join("RcloneBrowser.ini"),
        parent.join("rclone-browser.ini"),
    ];
    markers
        .iter()
        .any(|marker| marker.is_file())
        .then(|| parent.join("Rclone Browser Data"))
}

fn legacy_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/io.github.rclone-browser"))
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn persist_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("The data path has no parent directory.")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create the data directory: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not save {}: {error}", path.display()))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not commit {}: {error}", path.display()))
}

#[derive(Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    data: Value,
    error: Option<String>,
}

impl Response {
    fn success<T: Serialize>(data: T) -> Self {
        match serde_json::to_value(data) {
            Ok(data) => Self {
                ok: true,
                data,
                error: None,
            },
            Err(error) => Self::failure(format!("Could not encode the result: {error}")),
        }
    }

    fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: Value::Null,
            error: Some(error.into()),
        }
    }
}

#[unsafe(no_mangle)]
/// Runs one JSON request through the native backend and returns an owned JSON string.
///
/// # Safety
///
/// `request_json` must be null or point to a valid NUL-terminated C string. The
/// returned pointer must be released exactly once with [`rb_string_free`].
pub unsafe extern "C" fn rb_call(request_json: *const c_char) -> *mut c_char {
    let response = std::panic::catch_unwind(|| {
        if request_json.is_null() {
            return Response::failure("The native bridge received an empty request.");
        }
        let text = unsafe { CStr::from_ptr(request_json) }.to_string_lossy();
        match serde_json::from_str::<Request>(&text) {
            Ok(request) => match dispatch(request) {
                Ok(value) => Response::success(value),
                Err(error) => Response::failure(error),
            },
            Err(error) => Response::failure(format!("The native request is invalid: {error}")),
        }
    })
    .unwrap_or_else(|_| Response::failure("The Rust core recovered from an unexpected failure."));

    let encoded = serde_json::to_string(&response).unwrap_or_else(|_| {
        "{\"ok\":false,\"data\":null,\"error\":\"Could not encode the native response.\"}".into()
    });
    CString::new(encoded).unwrap().into_raw()
}

#[unsafe(no_mangle)]
/// Releases a string returned by [`rb_call`].
///
/// # Safety
///
/// `value` must be null or a pointer returned by [`rb_call`] that has not
/// already been released.
pub unsafe extern "C" fn rb_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

fn dispatch(request: Request) -> Result<Value, String> {
    match request.command.as_str() {
        "bootstrap" => json_value(bootstrap()),
        "listRemotes" => json_value(list_remotes()?),
        "listProviders" => json_value(list_providers()?),
        "startConfig" => json_value(start_config(parse(request.payload)?)?),
        "continueConfig" => json_value(continue_config(parse(request.payload)?)?),
        "cancelConfig" => {
            cancel_config(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "startUpdate" => json_value(start_update(parse(request.payload)?)?),
        "continueUpdate" => json_value(continue_update(parse(request.payload)?)?),
        "cancelUpdate" => {
            cancel_update(parse(request.payload)?);
            Ok(Value::Null)
        }
        "deleteRemote" => {
            delete_remote(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "listEntries" => json_value(list_entries(parse(request.payload)?)?),
        "createFolder" => {
            create_folder(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "renameEntry" => {
            rename_entry(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "moveEntry" => {
            move_entry(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "deleteEntry" => {
            delete_entry(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "publicLink" => json_value(public_link(parse(request.payload)?)?),
        "directorySize" => json_value(directory_size(parse(request.payload)?)?),
        "directoryTree" => json_value(directory_tree(parse(request.payload)?)?),
        "exportListing" => json_value(export_listing(parse(request.payload)?)?),
        "configFile" => json_value(config_file()?),
        "checkRcloneUpdate" => json_value(check_rclone_update()?),
        "saveSettings" => {
            save_settings(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "setPassword" => {
            set_password(parse(request.payload)?);
            Ok(Value::Null)
        }
        "copyCommand" => json_value(copy_command(parse(request.payload)?)),
        "taskCommand" => json_value(task_command(parse(request.payload)?)),
        "startTransfer" => json_value(start_transfer(parse(request.payload)?)?),
        "listTransfers" => json_value(sorted_transfers()),
        "cancelTransfer" => {
            cancel_work(parse(request.payload)?, true)?;
            Ok(Value::Null)
        }
        "clearFinishedTransfers" | "clearFinishedWork" => {
            clear_finished_work();
            Ok(Value::Null)
        }
        "startMount" => json_value(start_mount(parse(request.payload)?)?),
        "startStream" => json_value(start_stream(parse(request.payload)?)?),
        "listActivities" => json_value(sorted_activities()),
        "cancelActivity" => {
            cancel_work(parse(request.payload)?, false)?;
            Ok(Value::Null)
        }
        "cancelAll" => {
            cancel_all();
            Ok(Value::Null)
        }
        "listTasks" => json_value(lock(&state().tasks).clone()),
        "saveTask" => json_value(save_task(parse(request.payload)?)?),
        "deleteTask" => {
            delete_task(parse(request.payload)?)?;
            Ok(Value::Null)
        }
        "runTask" => json_value(run_task(parse(request.payload)?)?),
        "startTask" => json_value(start_task(parse(request.payload)?)?),
        other => Err(format!("Unknown native command '{other}'.")),
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid command parameters: {error}"))
}

fn json_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("Could not encode native data: {error}"))
}

fn bootstrap() -> Bootstrap {
    let status = rclone_status();
    let remotes = if status.available {
        list_remotes().unwrap_or_else(|_| local_remote())
    } else {
        local_remote()
    };
    Bootstrap {
        app_version: APP_VERSION.into(),
        settings: lock(&state().settings).clone(),
        rclone: status,
        remotes,
        transfers: sorted_transfers(),
        activities: sorted_activities(),
        tasks: lock(&state().tasks).clone(),
        data_directory: state().data_dir.to_string_lossy().into_owned(),
    }
}

fn rclone_status() -> RcloneStatus {
    match rclone_output(&["version".into()]) {
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

fn local_remote() -> Vec<Remote> {
    vec![Remote {
        name: "__local__".into(),
        remote_type: "local".into(),
        description: "Files on this Mac".into(),
        is_local: true,
        display_name: "On My Mac".into(),
    }]
}

fn list_remotes() -> Result<Vec<Remote>, String> {
    let output = rclone_output(&["listremotes".into(), "--long".into(), "--json".into()])?;
    let mut remotes: Vec<Remote> = serde_json::from_str(&output)
        .map_err(|error| format!("rclone returned invalid remote data: {error}"))?;
    for remote in &mut remotes {
        remote.display_name = remote.name.clone();
    }
    remotes.extend(local_remote());
    remotes.sort_by(|left, right| {
        right.is_local.cmp(&left.is_local).then_with(|| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
        })
    });
    Ok(remotes)
}

fn list_providers() -> Result<Vec<ConfigProvider>, String> {
    let output = rclone_output(&["config".into(), "providers".into()])?;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartConfigArgs {
    name: String,
    provider: String,
}

fn validate_config(name: &str, provider: &str) -> Result<(), String> {
    if name.trim().is_empty() {
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

fn start_config(args: StartConfigArgs) -> Result<ConfigQuestion, String> {
    validate_config(&args.name, &args.provider)?;
    if list_remotes()?
        .iter()
        .any(|remote| remote.name == args.name.trim())
    {
        return Err(format!(
            "A location named '{}' already exists.",
            args.name.trim()
        ));
    }
    lock(&state().pending_configs).insert(args.name.trim().into());
    let result = config_question(vec![
        "config".into(),
        "create".into(),
        args.name.trim().into(),
        args.provider.trim().into(),
        "--all".into(),
        "--non-interactive".into(),
    ]);
    if result
        .as_ref()
        .is_ok_and(|question| question.state.is_empty())
    {
        lock(&state().pending_configs).remove(args.name.trim());
    }
    result
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContinueConfigArgs {
    name: String,
    provider: String,
    state: String,
    result: String,
}

fn continue_config(args: ContinueConfigArgs) -> Result<ConfigQuestion, String> {
    validate_config(&args.name, &args.provider)?;
    if !lock(&state().pending_configs).contains(args.name.trim()) {
        return Err("This configuration session is no longer active.".into());
    }
    if args.state.trim().is_empty() {
        return Err("The rclone configuration session has already finished.".into());
    }
    let result = config_question(vec![
        "config".into(),
        "create".into(),
        args.name.trim().into(),
        args.provider.trim().into(),
        "--all".into(),
        "--non-interactive".into(),
        "--continue".into(),
        "--state".into(),
        args.state,
        "--result".into(),
        args.result,
    ]);
    if result
        .as_ref()
        .is_ok_and(|question| question.state.is_empty())
    {
        lock(&state().pending_configs).remove(args.name.trim());
    }
    result
}

fn config_question(arguments: Vec<String>) -> Result<ConfigQuestion, String> {
    let output = rclone_output(&arguments)?;
    let question: ConfigQuestion = serde_json::from_str(&output)
        .map_err(|error| format!("rclone returned an invalid configuration question: {error}"))?;
    if !question.error.trim().is_empty() {
        Err(question.error.clone())
    } else {
        Ok(question)
    }
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}

fn cancel_config(args: NameArgs) -> Result<(), String> {
    if lock(&state().pending_configs).remove(args.name.trim()) {
        let _ = rclone_output(&["config".into(), "delete".into(), args.name.trim().into()]);
    }
    Ok(())
}

fn delete_remote(args: NameArgs) -> Result<(), String> {
    if args.name.trim().is_empty() || args.name == "__local__" {
        return Err("Choose a configured rclone location.".into());
    }
    rclone_output(&["config".into(), "delete".into(), args.name.trim().into()]).map(|_| ())
}

fn start_update(args: NameArgs) -> Result<ConfigQuestion, String> {
    if args.name.trim().is_empty() || args.name == "__local__" {
        return Err("Choose a configured rclone location.".into());
    }
    if !list_remotes()?
        .iter()
        .any(|remote| remote.name == args.name)
    {
        return Err("That rclone location no longer exists.".into());
    }
    lock(&state().pending_updates).insert(args.name.clone());
    let result = config_question(vec![
        "config".into(),
        "update".into(),
        args.name.clone(),
        "--all".into(),
        "--non-interactive".into(),
    ]);
    if result
        .as_ref()
        .is_ok_and(|question| question.state.is_empty())
    {
        lock(&state().pending_updates).remove(&args.name);
    }
    result
}

#[derive(Deserialize)]
struct ContinueUpdateArgs {
    name: String,
    state: String,
    result: String,
}

fn continue_update(args: ContinueUpdateArgs) -> Result<ConfigQuestion, String> {
    if !lock(&state().pending_updates).contains(&args.name) {
        return Err("This reconfiguration session is no longer active.".into());
    }
    if args.state.trim().is_empty() {
        return Err("The rclone reconfiguration session has already finished.".into());
    }
    let result = config_question(vec![
        "config".into(),
        "update".into(),
        args.name.clone(),
        "--all".into(),
        "--non-interactive".into(),
        "--continue".into(),
        "--state".into(),
        args.state,
        "--result".into(),
        args.result,
    ]);
    if result
        .as_ref()
        .is_ok_and(|question| question.state.is_empty())
    {
        lock(&state().pending_updates).remove(&args.name);
    }
    result
}

fn cancel_update(args: NameArgs) {
    lock(&state().pending_updates).remove(&args.name);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserArgs {
    remote: String,
    path: String,
    #[serde(default)]
    shared_with_me: bool,
}

fn list_entries(args: BrowserArgs) -> Result<Vec<Entry>, String> {
    let settings = lock(&state().settings).clone();
    let mut arguments = vec!["lsjson".into(), browser_target(&args.remote, &args.path)];
    if args.shared_with_me {
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
    let output = rclone_output(&arguments)?;
    let raw: Vec<RawEntry> = serde_json::from_str(&output)
        .map_err(|error| format!("rclone returned an invalid file listing: {error}"))?;
    let mut entries: Vec<Entry> = raw
        .into_iter()
        .map(|entry| Entry {
            path: browser_join(&args.remote, &args.path, &entry.name),
            name: entry.name,
            is_dir: entry.is_dir,
            size: (!entry.is_dir).then_some(entry.size.max(0) as u64),
            mod_time: (!entry.mod_time.is_empty()).then_some(entry.mod_time),
            mime_type: (!entry.mime_type.is_empty()).then_some(entry.mime_type),
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

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PathArgs {
    remote: String,
    path: String,
    shared_with_me: bool,
}

fn create_folder(args: PathArgs) -> Result<(), String> {
    let mut arguments = vec!["mkdir".into(), browser_target(&args.remote, &args.path)];
    push_shared_with_me(&mut arguments, args.shared_with_me);
    rclone_output(&arguments).map(|_| ())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameArgs {
    remote: String,
    path: String,
    new_name: String,
    #[serde(default)]
    shared_with_me: bool,
}

fn rename_entry(args: RenameArgs) -> Result<(), String> {
    if args.new_name.trim().is_empty() || args.new_name.contains(['/', '\\']) {
        return Err("The new name must be a single non-empty path component.".into());
    }
    let destination = if args.remote == "__local__" {
        Path::new(&args.path)
            .parent()
            .unwrap_or(Path::new("/"))
            .join(args.new_name.trim())
            .to_string_lossy()
            .into_owned()
    } else {
        join_path(&parent_path(&args.path), args.new_name.trim())
    };
    let mut arguments = vec![
        "moveto".into(),
        browser_target(&args.remote, &args.path),
        browser_target(&args.remote, &destination),
    ];
    push_shared_with_me(&mut arguments, args.shared_with_me);
    rclone_output(&arguments).map(|_| ())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveArgs {
    remote: String,
    source: String,
    destination: String,
    #[serde(default)]
    shared_with_me: bool,
}

fn move_entry(args: MoveArgs) -> Result<(), String> {
    if args.destination.trim().is_empty() {
        return Err("Choose a destination path.".into());
    }
    let mut arguments = vec![
        "moveto".into(),
        browser_target(&args.remote, &args.source),
        browser_target(&args.remote, &args.destination),
    ];
    push_shared_with_me(&mut arguments, args.shared_with_me);
    rclone_output(&arguments).map(|_| ())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteArgs {
    remote: String,
    path: String,
    is_dir: bool,
    #[serde(default)]
    shared_with_me: bool,
}

fn delete_entry(args: DeleteArgs) -> Result<(), String> {
    let command = if args.is_dir { "purge" } else { "deletefile" };
    let mut arguments = vec![command.into(), browser_target(&args.remote, &args.path)];
    push_shared_with_me(&mut arguments, args.shared_with_me);
    rclone_output(&arguments).map(|_| ())
}

fn public_link(args: BrowserArgs) -> Result<String, String> {
    let mut arguments = vec!["link".into(), browser_target(&args.remote, &args.path)];
    if args.shared_with_me {
        arguments.push("--drive-shared-with-me".into());
    }
    Ok(rclone_output(&arguments)?.trim().into())
}

fn directory_size(args: BrowserArgs) -> Result<DirectorySummary, String> {
    let mut arguments = vec![
        "size".into(),
        browser_target(&args.remote, &args.path),
        "--json".into(),
    ];
    if args.shared_with_me {
        arguments.push("--drive-shared-with-me".into());
    }
    serde_json::from_str(&rclone_output(&arguments)?)
        .map_err(|error| format!("rclone returned invalid size information: {error}"))
}

fn directory_tree(args: BrowserArgs) -> Result<String, String> {
    let mut arguments = vec![
        "tree".into(),
        "-d".into(),
        browser_target(&args.remote, &args.path),
    ];
    if args.shared_with_me {
        arguments.push("--drive-shared-with-me".into());
    }
    rclone_output(&arguments)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    remote: String,
    path: String,
    destination: String,
    format: String,
    #[serde(default)]
    shared_with_me: bool,
}

fn export_listing(args: ExportArgs) -> Result<u64, String> {
    let options = lock(&state().settings).export_options.clone();
    let mut arguments = vec![
        "lsjson".into(),
        browser_target(&args.remote, &args.path),
        "--recursive".into(),
        "--files-only".into(),
    ];
    if args.shared_with_me {
        arguments.push("--drive-shared-with-me".into());
    }
    if options.one_file_system {
        arguments.push("--one-file-system".into());
    }
    push_option(&mut arguments, "--min-size", &options.min_size);
    push_option(&mut arguments, "--min-age", &options.min_age);
    push_option(&mut arguments, "--max-age", &options.max_age);
    if options.max_depth > 0 {
        arguments.extend(["--max-depth".into(), options.max_depth.to_string()]);
    }
    for value in options
        .excludes
        .iter()
        .filter(|value| !value.trim().is_empty())
    {
        arguments.extend(["--exclude".into(), value.trim().into()]);
    }
    arguments.extend(options.extra_args);
    let entries: Vec<RawEntry> = serde_json::from_str(&rclone_output(&arguments)?)
        .map_err(|error| format!("rclone returned an invalid export listing: {error}"))?;
    let mut text = String::new();
    if args.format.eq_ignore_ascii_case("csv") {
        text.push_str("Path,Modified,Size\n");
    }
    for entry in &entries {
        let name = if entry.path.is_empty() {
            &entry.name
        } else {
            &entry.path
        };
        if args.format.eq_ignore_ascii_case("csv") {
            text.push_str(&format!(
                "{},{},{}\n",
                csv_field(name),
                csv_field(&entry.mod_time),
                entry.size.max(0)
            ));
        } else {
            text.push_str(name);
            text.push('\n');
        }
    }
    fs::write(&args.destination, text)
        .map_err(|error| format!("Could not write '{}': {error}", args.destination))?;
    Ok(entries.len() as u64)
}

fn config_file() -> Result<String, String> {
    let output = rclone_output(&["config".into(), "file".into()])?;
    output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned())
        .ok_or("rclone did not report a configuration path.".into())
}

fn check_rclone_update() -> Result<RcloneUpdateInfo, String> {
    let output = rclone_output(&["selfupdate".into(), "--check".into()])?;
    parse_rclone_update(&output)
}

fn save_settings(settings: Settings) -> Result<(), String> {
    if settings.rclone_path.trim().is_empty() {
        return Err("The rclone executable path cannot be empty.".into());
    }
    persist_json(&state().data_dir.join("settings.json"), &settings)?;
    *lock(&state().settings) = settings;
    Ok(())
}

#[derive(Deserialize)]
struct PasswordArgs {
    password: String,
}

fn set_password(args: PasswordArgs) {
    *lock(&state().password) = (!args.password.is_empty()).then_some(args.password);
}

fn copy_command(request: TransferRequest) -> String {
    let operation = match request.operation {
        TransferOperation::Copy if !request.is_directory => "copyto",
        TransferOperation::Move if !request.is_directory => "moveto",
        TransferOperation::Copy => "copy",
        TransferOperation::Move => "move",
        TransferOperation::Sync => "sync",
    };
    let mut arguments = vec![operation.into()];
    if request.is_directory {
        arguments.push("--create-empty-src-dirs".into());
    }
    arguments.extend(request.extra_args);
    arguments.extend([request.source, request.destination]);
    display_command(arguments)
}

fn task_command(args: StartTaskArgs) -> String {
    display_command(task_arguments(&args.task, args.dry_run))
}

fn display_command(arguments: Vec<String>) -> String {
    let settings = lock(&state().settings).clone();
    let mut parts = vec![settings.rclone_path];
    if let Some(config) = settings
        .config_path
        .filter(|value| !value.trim().is_empty())
    {
        parts.extend(["--config".into(), config]);
    }
    parts.extend(settings.advanced_args);
    parts.extend(arguments);
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._/:=@+".contains(character))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn start_transfer(request: TransferRequest) -> Result<TransferSnapshot, String> {
    if request.source.trim().is_empty() || request.destination.trim().is_empty() {
        return Err("Both source and destination are required.".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let snapshot = TransferSnapshot::new(id.clone(), &request);
    lock(&state().transfers).insert(id.clone(), snapshot.clone());
    thread::spawn(move || run_transfer(id, request));
    Ok(snapshot)
}

fn run_transfer(id: String, request: TransferRequest) {
    if lock(&state().transfers)
        .get(&id)
        .is_some_and(|snapshot| snapshot.status == WorkStatus::Cancelled)
    {
        return;
    }
    update_transfer(&id, |snapshot| snapshot.status = WorkStatus::Running);
    let operation = match request.operation {
        TransferOperation::Copy if !request.is_directory => "copyto",
        TransferOperation::Move if !request.is_directory => "moveto",
        TransferOperation::Copy => "copy",
        TransferOperation::Move => "move",
        TransferOperation::Sync => "sync",
    };
    let mut arguments = vec![
        operation.into(),
        request.source,
        request.destination,
        "--use-json-log".into(),
        "--stats".into(),
        "500ms".into(),
        "--stats-log-level".into(),
        "NOTICE".into(),
        "--log-level".into(),
        "NOTICE".into(),
    ];
    if request.is_directory {
        arguments.push("--create-empty-src-dirs".into());
    }
    arguments.extend(request.extra_args);
    let mut command = rclone_command(&arguments);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    match command.spawn() {
        Ok(mut child) => {
            lock(&state().processes).insert(id.clone(), vec![child.id()]);
            if let Some(stderr) = child.stderr.take() {
                let reader_id = id.clone();
                thread::spawn(move || read_transfer_output(reader_id, stderr));
            }
            match child.wait() {
                Ok(status) => finish_transfer(&id, status.success(), status.to_string()),
                Err(error) => finish_transfer(&id, false, error.to_string()),
            }
        }
        Err(error) => finish_transfer(&id, false, format!("Could not start rclone: {error}")),
    }
    lock(&state().processes).remove(&id);
}

fn read_transfer_output(id: String, stderr: impl std::io::Read) {
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        update_transfer(&id, |snapshot| {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                let stats = value.get("stats").unwrap_or(&value);
                snapshot.bytes =
                    json_u64(stats, &["bytes", "transferredBytes"]).unwrap_or(snapshot.bytes);
                snapshot.total_bytes = json_u64(stats, &["totalBytes"]).or(snapshot.total_bytes);
                snapshot.speed = json_f64(stats, &["speed", "bytesPerSecond"]).or(snapshot.speed);
                snapshot.eta_seconds = json_f64(stats, &["eta", "etaSeconds"])
                    .filter(|eta| *eta >= 0.0)
                    .or(snapshot.eta_seconds);
                snapshot.checks = json_u64(stats, &["checks"]).unwrap_or(snapshot.checks);
                snapshot.total_checks = json_u64(stats, &["totalChecks"]).or(snapshot.total_checks);
                snapshot.files_transferred = json_u64(stats, &["transfers", "filesTransferred"])
                    .unwrap_or(snapshot.files_transferred);
                snapshot.total_files =
                    json_u64(stats, &["totalTransfers", "totalFiles"]).or(snapshot.total_files);
                snapshot.errors = json_u64(stats, &["errors"]).unwrap_or(snapshot.errors);
                snapshot.elapsed_seconds =
                    json_f64(stats, &["elapsedTime", "elapsed"]).or(snapshot.elapsed_seconds);
                if let Some(message) = value.get("msg").and_then(Value::as_str) {
                    push_log(&mut snapshot.log_tail, message);
                }
            } else {
                push_log(&mut snapshot.log_tail, &line);
            }
        });
    }
}

fn finish_transfer(id: &str, succeeded: bool, detail: String) {
    update_transfer(id, |snapshot| {
        if snapshot.status != WorkStatus::Cancelled {
            snapshot.status = if succeeded {
                WorkStatus::Completed
            } else {
                WorkStatus::Failed
            };
            if !succeeded {
                snapshot.error = snapshot.log_tail.last().cloned().or(Some(detail));
            }
        }
        snapshot.finished_at = Some(unix_timestamp());
    });
}

fn update_transfer(id: &str, update: impl FnOnce(&mut TransferSnapshot)) {
    if let Some(snapshot) = lock(&state().transfers).get_mut(id) {
        update(snapshot);
    }
}

fn sorted_transfers() -> Vec<TransferSnapshot> {
    let mut values: Vec<_> = lock(&state().transfers).values().cloned().collect();
    values.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.started_at));
    values
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

fn cancel_work(args: IdArgs, transfer: bool) -> Result<(), String> {
    let process_ids = lock(&state().processes)
        .get(&args.id)
        .cloned()
        .unwrap_or_default();
    let mut found_running = false;
    let mut mount_destination = None;
    if transfer {
        if let Some(snapshot) = lock(&state().transfers).get_mut(&args.id)
            && matches!(snapshot.status, WorkStatus::Queued | WorkStatus::Running)
        {
            found_running = true;
            snapshot.status = WorkStatus::Cancelled;
            snapshot.finished_at = Some(unix_timestamp());
        }
    } else if let Some(snapshot) = lock(&state().activities).get_mut(&args.id)
        && matches!(snapshot.status, WorkStatus::Queued | WorkStatus::Running)
    {
        found_running = true;
        if snapshot.kind == ActivityKind::Mount {
            mount_destination = Some(snapshot.destination.clone());
        }
        snapshot.status = WorkStatus::Cancelled;
        snapshot.finished_at = Some(unix_timestamp());
    }
    if !found_running {
        return Err("That operation is no longer running.".into());
    }
    for process_id in process_ids {
        let _ = Command::new("/bin/kill")
            .args(["-TERM", &process_id.to_string()])
            .status();
    }
    if let Some(destination) = mount_destination {
        unmount(&destination);
    }
    Ok(())
}

fn cancel_all() {
    let mount_destinations: Vec<String> = lock(&state().activities)
        .values()
        .filter(|snapshot| snapshot.status.is_running() && snapshot.kind == ActivityKind::Mount)
        .map(|snapshot| snapshot.destination.clone())
        .collect();
    let running: Vec<(String, Vec<u32>)> = lock(&state().processes)
        .iter()
        .map(|(id, process_ids)| (id.clone(), process_ids.clone()))
        .collect();
    for (id, process_ids) in running {
        update_transfer(&id, |snapshot| {
            snapshot.status = WorkStatus::Cancelled;
            snapshot.finished_at = Some(unix_timestamp());
        });
        update_activity(&id, |snapshot| {
            snapshot.status = WorkStatus::Cancelled;
            snapshot.finished_at = Some(unix_timestamp());
        });
        for process_id in process_ids {
            let _ = Command::new("/bin/kill")
                .args(["-TERM", &process_id.to_string()])
                .status();
        }
    }
    for snapshot in lock(&state().transfers).values_mut() {
        if snapshot.status.is_running() {
            snapshot.status = WorkStatus::Cancelled;
            snapshot.finished_at = Some(unix_timestamp());
        }
    }
    for snapshot in lock(&state().activities).values_mut() {
        if snapshot.status.is_running() {
            snapshot.status = WorkStatus::Cancelled;
            snapshot.finished_at = Some(unix_timestamp());
        }
    }
    for destination in mount_destinations {
        unmount(&destination);
    }
}

fn clear_finished_work() {
    lock(&state().transfers)
        .retain(|_, snapshot| matches!(snapshot.status, WorkStatus::Queued | WorkStatus::Running));
    lock(&state().activities)
        .retain(|_, snapshot| matches!(snapshot.status, WorkStatus::Queued | WorkStatus::Running));
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MountArgs {
    source: String,
    destination: String,
    #[serde(default)]
    extra_args: Vec<String>,
}

fn start_mount(args: MountArgs) -> Result<ActivitySnapshot, String> {
    if args.source.trim().is_empty() || args.destination.trim().is_empty() {
        return Err("Choose a source and a mount location.".into());
    }
    fs::create_dir_all(&args.destination)
        .map_err(|error| format!("Could not create the mount directory: {error}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let snapshot = ActivitySnapshot {
        id: id.clone(),
        kind: ActivityKind::Mount,
        source: args.source.clone(),
        destination: args.destination.clone(),
        status: WorkStatus::Queued,
        started_at: unix_timestamp(),
        finished_at: None,
        error: None,
        log_tail: Vec::new(),
    };
    lock(&state().activities).insert(id.clone(), snapshot.clone());
    thread::spawn(move || run_mount(id, args));
    Ok(snapshot)
}

fn run_mount(id: String, args: MountArgs) {
    if lock(&state().activities)
        .get(&id)
        .is_some_and(|snapshot| snapshot.status == WorkStatus::Cancelled)
    {
        return;
    }
    let mut arguments = vec!["mount".into(), args.source, args.destination.clone()];
    let configured = lock(&state().settings).mount_args.clone();
    arguments.extend(configured);
    arguments.extend(args.extra_args);
    update_activity(&id, |snapshot| snapshot.status = WorkStatus::Running);
    let mut command = rclone_command(&arguments);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    match command.spawn() {
        Ok(mut child) => {
            lock(&state().processes).insert(id.clone(), vec![child.id()]);
            if let Some(stderr) = child.stderr.take() {
                let reader_id = id.clone();
                thread::spawn(move || read_activity_output(reader_id, stderr));
            }
            match child.wait() {
                Ok(status) => finish_activity(&id, status.success(), status.to_string()),
                Err(error) => finish_activity(&id, false, error.to_string()),
            }
        }
        Err(error) => finish_activity(&id, false, format!("Could not start rclone mount: {error}")),
    }
    lock(&state().processes).remove(&id);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamArgs {
    source: String,
    #[serde(default)]
    command: String,
}

fn start_stream(mut args: StreamArgs) -> Result<ActivitySnapshot, String> {
    if args.source.trim().is_empty() {
        return Err("Choose a file to stream.".into());
    }
    if args.command.trim().is_empty() {
        args.command = lock(&state().settings).stream_command.clone();
    }
    if args.command.trim().is_empty() {
        return Err("Set a stream player command in Settings.".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let snapshot = ActivitySnapshot {
        id: id.clone(),
        kind: ActivityKind::Stream,
        source: args.source.clone(),
        destination: args.command.clone(),
        status: WorkStatus::Queued,
        started_at: unix_timestamp(),
        finished_at: None,
        error: None,
        log_tail: Vec::new(),
    };
    lock(&state().activities).insert(id.clone(), snapshot.clone());
    thread::spawn(move || run_stream(id, args));
    Ok(snapshot)
}

fn run_stream(id: String, args: StreamArgs) {
    if lock(&state().activities)
        .get(&id)
        .is_some_and(|snapshot| snapshot.status == WorkStatus::Cancelled)
    {
        return;
    }
    update_activity(&id, |snapshot| snapshot.status = WorkStatus::Running);
    let mut rclone = rclone_command(&["cat".into(), args.source]);
    rclone
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let result = (|| -> Result<(), String> {
        let player_parts = split_command(&args.command)?;
        let (player_program, player_arguments) = player_parts
            .split_first()
            .ok_or("Set a stream player command in Settings.")?;
        let mut source = rclone
            .spawn()
            .map_err(|error| format!("Could not start rclone stream: {error}"))?;
        if let Some(stderr) = source.stderr.take() {
            let reader_id = id.clone();
            thread::spawn(move || read_activity_output(reader_id, stderr));
        }
        let stream = source
            .stdout
            .take()
            .ok_or("Could not connect the stream output.")?;
        let mut player = Command::new(player_program)
            .args(player_arguments)
            .stdin(Stdio::from(stream))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start the stream player: {error}"))?;
        lock(&state().processes).insert(id.clone(), vec![source.id(), player.id()]);
        if let Some(stderr) = player.stderr.take() {
            let reader_id = id.clone();
            thread::spawn(move || read_activity_output(reader_id, stderr));
        }
        let player_status = player.wait().map_err(|error| error.to_string())?;
        let _ = source.kill();
        let _ = source.wait();
        if player_status.success() {
            Ok(())
        } else {
            Err(format!("The stream player exited with {player_status}."))
        }
    })();
    match result {
        Ok(()) => finish_activity(&id, true, String::new()),
        Err(error) => finish_activity(&id, false, error),
    }
    lock(&state().processes).remove(&id);
}

fn read_activity_output(id: String, output: impl std::io::Read) {
    for line in BufReader::new(output).lines().map_while(Result::ok) {
        update_activity(&id, |snapshot| push_log(&mut snapshot.log_tail, &line));
    }
}

fn update_activity(id: &str, update: impl FnOnce(&mut ActivitySnapshot)) {
    if let Some(snapshot) = lock(&state().activities).get_mut(id) {
        update(snapshot);
    }
}

fn finish_activity(id: &str, succeeded: bool, error: String) {
    update_activity(id, |snapshot| {
        if snapshot.status != WorkStatus::Cancelled {
            snapshot.status = if succeeded {
                WorkStatus::Completed
            } else {
                WorkStatus::Failed
            };
            if !succeeded {
                snapshot.error = snapshot.log_tail.last().cloned().or(Some(error));
            }
        }
        snapshot.finished_at = Some(unix_timestamp());
    });
}

fn sorted_activities() -> Vec<ActivitySnapshot> {
    let mut values: Vec<_> = lock(&state().activities).values().cloned().collect();
    values.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.started_at));
    values
}

fn unmount(destination: &str) {
    let result = Command::new("/usr/sbin/diskutil")
        .args(["unmount", destination])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !result.is_ok_and(|status| status.success()) {
        let _ = Command::new("/sbin/umount")
            .arg(destination)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn save_task(mut task: SavedTask) -> Result<SavedTask, String> {
    if task.description.trim().is_empty() {
        return Err("A task name is required.".into());
    }
    if task.source.trim().is_empty() || task.destination.trim().is_empty() {
        return Err("Both source and destination are required.".into());
    }
    if task.id.is_empty() {
        task.id = uuid::Uuid::new_v4().to_string();
    }
    let mut tasks = lock(&state().tasks);
    if let Some(existing) = tasks.iter_mut().find(|existing| existing.id == task.id) {
        *existing = task.clone();
    } else {
        tasks.push(task.clone());
    }
    persist_json(&state().data_dir.join("tasks.json"), &*tasks)?;
    Ok(task)
}

fn delete_task(args: IdArgs) -> Result<(), String> {
    let mut tasks = lock(&state().tasks);
    tasks.retain(|task| task.id != args.id);
    persist_json(&state().data_dir.join("tasks.json"), &*tasks)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunTaskArgs {
    id: String,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartTaskArgs {
    task: SavedTask,
    #[serde(default)]
    dry_run: bool,
}

fn run_task(args: RunTaskArgs) -> Result<TransferSnapshot, String> {
    let task = lock(&state().tasks)
        .iter()
        .find(|task| task.id == args.id)
        .cloned()
        .ok_or("The saved task no longer exists.")?;
    start_task(StartTaskArgs {
        task,
        dry_run: args.dry_run,
    })
}

fn start_task(args: StartTaskArgs) -> Result<TransferSnapshot, String> {
    if args.task.source.trim().is_empty() || args.task.destination.trim().is_empty() {
        return Err("Both source and destination are required.".into());
    }
    let mut extra_args = task_arguments(&args.task, args.dry_run);
    let request = TransferRequest {
        direction: args.task.direction,
        operation: args.task.operation,
        source: args.task.source,
        destination: args.task.destination,
        is_directory: args.task.is_directory,
        extra_args: {
            extra_args.shrink_to_fit();
            extra_args
        },
        label: (!args.task.description.trim().is_empty()).then_some(args.task.description),
    };
    start_transfer(request)
}

fn task_arguments(task: &SavedTask, dry_run: bool) -> Vec<String> {
    let mut args = Vec::new();
    if dry_run {
        args.push("--dry-run".into());
    }
    if task.operation == TransferOperation::Sync
        && let Some(mode) = task.sync_delete_mode
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
    if task.update {
        args.push("--update".into());
    }
    if task.ignore_existing {
        args.push("--ignore-existing".into());
    }
    match task.compare_mode {
        CompareMode::Checksum => args.push("--checksum".into()),
        CompareMode::IgnoreSize => args.push("--ignore-size".into()),
        CompareMode::SizeOnly => args.push("--size-only".into()),
        CompareMode::ChecksumIgnoreSize => {
            args.extend(["--checksum".into(), "--ignore-size".into()])
        }
        CompareMode::SizeAndModTime => {}
    }
    if task.one_file_system {
        args.push("--one-file-system".into());
    }
    if task.no_update_modtime {
        args.push("--no-update-modtime".into());
    }
    args.extend([
        "--transfers".into(),
        task.transfers.to_string(),
        "--checkers".into(),
        task.checkers.to_string(),
    ]);
    push_option(&mut args, "--bwlimit", &task.bandwidth);
    push_option(&mut args, "--min-size", &task.min_size);
    push_option(&mut args, "--min-age", &task.min_age);
    push_option(&mut args, "--max-age", &task.max_age);
    if task.max_depth > 0 {
        args.extend(["--max-depth".into(), task.max_depth.to_string()]);
    }
    args.extend([
        "--contimeout".into(),
        format!("{}s", task.connect_timeout_seconds),
        "--timeout".into(),
        format!("{}s", task.idle_timeout_seconds),
        "--retries".into(),
        task.retries.to_string(),
        "--low-level-retries".into(),
        task.low_level_retries.to_string(),
    ]);
    if task.delete_excluded {
        args.push("--delete-excluded".into());
    }
    for value in task
        .excludes
        .iter()
        .filter(|value| !value.trim().is_empty())
    {
        args.extend(["--exclude".into(), value.trim().into()]);
    }
    args.extend(task.extra_args.clone());
    if task.shared_with_me {
        args.push("--drive-shared-with-me".into());
    }
    args
}

fn rclone_command(arguments: &[String]) -> Command {
    let settings = lock(&state().settings).clone();
    let password = lock(&state().password).clone();
    let mut command = Command::new(resolve_rclone_path(&settings.rclone_path));
    if let Some(config) = settings
        .config_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        command.args(["--config", config]);
    }
    command.args(&settings.advanced_args).args(arguments);
    if let Some(password) = password.filter(|value| !value.is_empty()) {
        command.env("RCLONE_CONFIG_PASS", password);
    }
    if settings.use_proxy {
        set_env(&mut command, "HTTP_PROXY", &settings.http_proxy);
        set_env(&mut command, "HTTPS_PROXY", &settings.https_proxy);
        set_env(&mut command, "NO_PROXY", &settings.no_proxy);
    }
    command
}

fn resolve_rclone_path(configured: &str) -> PathBuf {
    let path = PathBuf::from(configured);
    if path.components().count() > 1 || path.is_absolute() {
        return path;
    }
    for candidate in [
        "/opt/homebrew/bin/rclone",
        "/usr/local/bin/rclone",
        "/usr/bin/rclone",
    ] {
        if Path::new(candidate).is_file() {
            return candidate.into();
        }
    }
    path
}

fn rclone_output(arguments: &[String]) -> Result<String, String> {
    let settings = lock(&state().settings).clone();
    let output = rclone_command(arguments)
        .stdin(Stdio::null())
        .output()
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

fn set_env(command: &mut Command, key: &str, value: &str) {
    if !value.trim().is_empty() {
        command.env(key, value.trim());
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

fn browser_target(remote: &str, path: &str) -> String {
    if remote == "__local__" {
        path.into()
    } else if path.is_empty() {
        format!("{remote}:")
    } else {
        format!("{remote}:{}", path.trim_start_matches('/'))
    }
}

fn browser_join(remote: &str, base: &str, name: &str) -> String {
    if remote == "__local__" {
        Path::new(base).join(name).to_string_lossy().into_owned()
    } else {
        join_path(base, name)
    }
}

fn join_path(base: &str, name: &str) -> String {
    if base.trim_matches('/').is_empty() {
        name.trim_matches('/').into()
    } else {
        format!("{}/{}", base.trim_matches('/'), name.trim_matches('/'))
    }
}

fn parent_path(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent.into())
        .unwrap_or_default()
}

fn push_option(arguments: &mut Vec<String>, name: &str, value: &str) {
    if !value.trim().is_empty() {
        arguments.extend([name.into(), value.trim().into()]);
    }
}

fn push_shared_with_me(arguments: &mut Vec<String>, enabled: bool) {
    if enabled {
        arguments.push("--drive-shared-with-me".into());
    }
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn push_log(log: &mut Vec<String>, line: &str) {
    if line.trim().is_empty() {
        return;
    }
    log.push(line.trim().into());
    if log.len() > 80 {
        log.drain(..log.len() - 80);
    }
}

fn json_u64(value: &Value, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| value.get(name).and_then(Value::as_u64))
}
fn json_f64(value: &Value, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| value.get(name).and_then(Value::as_f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_local_and_remote_paths() {
        assert_eq!(browser_target("drive", "Folder/File"), "drive:Folder/File");
        assert_eq!(browser_target("drive", ""), "drive:");
        assert_eq!(browser_target("__local__", "/tmp"), "/tmp");
    }

    #[test]
    fn rejects_bad_location_names() {
        assert!(validate_config("", "drive").is_err());
        assert!(validate_config("bad:name", "drive").is_err());
        assert!(validate_config("Cloud", "drive").is_ok());
    }

    #[test]
    fn parent_paths_are_stable() {
        assert_eq!(parent_path("Folder/Subfolder/File"), "Folder/Subfolder");
        assert_eq!(parent_path("File"), "");
    }

    #[test]
    fn splits_stream_player_commands_without_a_shell() {
        assert_eq!(
            split_command("/Applications/Test\\ Player --title 'Cloud video' -").unwrap(),
            vec!["/Applications/Test Player", "--title", "Cloud video", "-"]
        );
        assert!(split_command("player 'unfinished").is_err());
    }

    #[test]
    fn quotes_shell_commands_for_copying() {
        assert_eq!(shell_quote("rclone"), "rclone");
        assert_eq!(shell_quote("/tmp/a file"), "'/tmp/a file'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn parses_stable_and_beta_update_channels() {
        let info = parse_rclone_update(
            "yours: 1.74.4\nlatest: 1.75.0 (released 2026-07-31)\n\
             upgrade: https://downloads.rclone.org/v1.75.0\nbeta:\n\
             1.76.0-beta.10147.f0b210a88 (released 2026-08-14)\n\
             upgrade: https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88\n",
        )
        .unwrap();
        assert_eq!(info.current_version, "1.74.4");
        assert_eq!(info.stable.as_ref().unwrap().version, "1.75.0");
        assert_eq!(
            info.stable.as_ref().unwrap().released.as_deref(),
            Some("2026-07-31")
        );
        assert_eq!(
            info.beta.as_ref().unwrap().version,
            "1.76.0-beta.10147.f0b210a88"
        );
        assert!(
            info.beta
                .as_ref()
                .unwrap()
                .download_url
                .as_deref()
                .unwrap()
                .starts_with("https://beta.rclone.org/")
        );
    }

    #[test]
    fn derives_official_download_links_when_rclone_omits_them() {
        let info = parse_rclone_update(
            "yours: 1.74.4\nlatest: 1.75.0 (released 2026-07-31)\n\
             beta: 1.76.0-beta.10147.f0b210a88 (released 2026-08-14)\n",
        )
        .unwrap();
        assert_eq!(
            info.stable.unwrap().download_url.as_deref(),
            Some("https://downloads.rclone.org/v1.75.0")
        );
        assert_eq!(
            info.beta.unwrap().download_url.as_deref(),
            Some("https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88")
        );
    }
}
