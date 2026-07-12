use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::fs::File;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(not(target_os = "macos"))]
use tauri_plugin_dialog::DialogExt;

use super::{get_settings_path, normalize_pill_background_color, AppSettings};
use crate::events::EventName;
use crate::shortcut::{canonicalize_hotkey_string, ShortcutAction, ShortcutProfile};
use crate::state::app_state::AppState;

const SETTINGS_EXPORT_FORMAT: &str = "ariatype-settings";
const SETTINGS_EXPORT_SCHEMA_VERSION: u32 = 1;
const SETTINGS_EXPORT_FILE_NAME: &str = "ariatype-settings.json";
const SETTINGS_IMPORT_ENVELOPE_FIELDS: &[&str] = &[
    "format",
    "schema_version",
    "app_version",
    "exported_at",
    "secrets_included",
    "device_specific_fields_included",
    "settings",
];
const RAW_SETTINGS_OPTIONAL_FIELDS: &[&str] = &[
    "stay_in_tray",
    "window_context_enabled",
    "pill_size",
    "pill_background_color",
    "pill_background_opacity",
    "correction_memory_enabled",
];
pub(super) const MAX_SETTINGS_IMPORT_BYTES: usize = 2 * 1024 * 1024;

const V1_SETTINGS_FIELDS: &[&str] = &[
    "shortcut_profiles",
    "recording_mode",
    "model",
    "stt_engine",
    "pill_position",
    "pill_indicator_mode",
    "auto_start",
    "gpu_acceleration",
    "language",
    "stt_engine_language",
    "beep_on_record",
    "audio_device",
    "polish_system_prompt",
    "polish_model",
    "theme_mode",
    "stt_engine_initial_prompt",
    "model_resident",
    "idle_unload_minutes",
    "denoise_mode",
    "stt_engine_work_domain",
    "stt_engine_work_domain_prompt",
    "stt_engine_work_subdomain",
    "stt_engine_user_glossary",
    "custom_dictionary",
    "analytics_opt_in",
    "cloud_stt_enabled",
    "active_cloud_stt_provider",
    "cloud_stt_configs",
    "cloud_polish_enabled",
    "active_cloud_polish_provider",
    "cloud_polish_configs",
    "local_polish_runtime",
    "polish_stream_direct_typing_enabled",
    "vad_enabled",
    "stay_in_tray",
    "polish_custom_templates",
    "window_context_enabled",
    "pill_size",
    "pill_background_color",
    "pill_background_opacity",
    "correction_memory_enabled",
];
const V1_CLOUD_STT_CONFIG_FIELDS: &[&str] = &[
    "enabled",
    "provider_type",
    "api_key",
    "app_id",
    "base_url",
    "model",
    "language",
];
const V1_CLOUD_POLISH_CONFIG_FIELDS: &[&str] = &[
    "enabled",
    "provider_type",
    "api_key",
    "base_url",
    "model",
    "enable_thinking",
];
const V1_LOCAL_RUNTIME_FIELDS: &[&str] = &[
    "provider_type",
    "base_url",
    "api_key",
    "server_command",
    "server_args_json",
    "ready_timeout_secs",
];
const V1_SHORTCUT_PROFILE_FIELDS: &[&str] = &["hotkey", "trigger_mode", "action"];
const V1_CUSTOM_TEMPLATE_FIELDS: &[&str] = &["id", "name", "system_prompt"];

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SettingsExportResult {
    Saved { file_path: String },
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SettingsImportSelectionResult {
    Selected { document: String },
    Cancelled,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsImportRestartStrategy {
    Automatic,
    Manual,
}

#[derive(Debug, Serialize)]
pub struct SettingsImportResult {
    pub restart_required: bool,
    pub restart_strategy: SettingsImportRestartStrategy,
}

pub(super) fn settings_import_restart_strategy(is_dev: bool) -> SettingsImportRestartStrategy {
    if is_dev {
        SettingsImportRestartStrategy::Manual
    } else {
        SettingsImportRestartStrategy::Automatic
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsImportEnvelope {
    format: String,
    schema_version: u32,
    app_version: String,
    exported_at: String,
    secrets_included: bool,
    device_specific_fields_included: bool,
    settings: serde_json::Value,
}

pub(super) fn export_settings_file_to_directory(
    source_path: &Path,
    destination_directory: &Path,
) -> Result<SettingsExportResult, String> {
    let source_metadata = fs::metadata(source_path)
        .map_err(|_| "failed to inspect the current settings file".to_string())?;
    validate_settings_export_source(&source_metadata)?;

    let mut source_options = OpenOptions::new();
    source_options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        source_options.custom_flags(libc::O_NONBLOCK);
    }
    let mut source = source_options
        .open(source_path)
        .map_err(|_| "failed to open the current settings file".to_string())?;
    let source_metadata = source
        .metadata()
        .map_err(|_| "failed to inspect the current settings file".to_string())?;
    validate_settings_export_source(&source_metadata)?;

    fs::create_dir_all(destination_directory)
        .map_err(|_| "failed to create the downloads directory".to_string())?;

    for sequence in 1..=u32::MAX {
        let file_name = settings_export_file_name(sequence);
        let destination = destination_directory.join(file_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut destination_file = match options.open(&destination) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("failed to create the settings export file".to_string()),
        };

        let copied = std::io::copy(&mut source, &mut destination_file);
        let copied_all_bytes = copied
            .map(|bytes| bytes == source_metadata.len())
            .unwrap_or(false);
        if !copied_all_bytes || destination_file.sync_all().is_err() {
            drop(destination_file);
            let _ = fs::remove_file(&destination);
            return Err("failed to copy the settings export file".to_string());
        }
        drop(destination_file);

        if let Err(error) = sync_parent_directory(&destination) {
            tracing::warn!(error = %error, "settings_export_directory_sync_failed");
        }

        return Ok(SettingsExportResult::Saved {
            file_path: destination.to_string_lossy().into_owned(),
        });
    }

    Err("failed to choose a settings export file name".to_string())
}

fn validate_settings_export_source(metadata: &fs::Metadata) -> Result<(), String> {
    if !metadata.is_file() {
        return Err("current settings path is not a regular file".to_string());
    }
    if metadata.len() > MAX_SETTINGS_IMPORT_BYTES as u64 {
        return Err("current settings file exceeds the 2 MiB limit".to_string());
    }
    Ok(())
}

fn settings_export_file_name(sequence: u32) -> String {
    if sequence == 1 {
        SETTINGS_EXPORT_FILE_NAME.to_string()
    } else {
        format!("ariatype-settings-{sequence}.json")
    }
}

pub(super) fn read_settings_import_document(path: &Path) -> Result<String, String> {
    let source_metadata =
        fs::metadata(path).map_err(|_| "failed to inspect settings backup".to_string())?;
    validate_settings_import_source(&source_metadata)?;

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "failed to open settings backup".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "failed to inspect settings backup".to_string())?;
    validate_settings_import_source(&metadata)?;

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_SETTINGS_IMPORT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "failed to read settings backup".to_string())?;
    if bytes.len() > MAX_SETTINGS_IMPORT_BYTES {
        return Err("settings backup exceeds the 2 MiB limit".to_string());
    }

    String::from_utf8(bytes).map_err(|_| "settings backup is not valid UTF-8".to_string())
}

fn validate_settings_import_source(metadata: &fs::Metadata) -> Result<(), String> {
    if !metadata.is_file() {
        return Err("invalid settings backup source".to_string());
    }
    if metadata.len() > MAX_SETTINGS_IMPORT_BYTES as u64 {
        return Err("settings backup exceeds the 2 MiB limit".to_string());
    }
    Ok(())
}

pub(super) fn select_settings_import_with_source<C, R>(
    choose_source: C,
    read_source: R,
) -> Result<SettingsImportSelectionResult, String>
where
    C: FnOnce() -> Result<Option<PathBuf>, String>,
    R: FnOnce(&Path) -> Result<String, String>,
{
    let Some(source) = choose_source()? else {
        return Ok(SettingsImportSelectionResult::Cancelled);
    };
    let document = read_source(&source)?;

    Ok(SettingsImportSelectionResult::Selected { document })
}

#[cfg(target_os = "macos")]
pub(super) fn require_settings_import_panel(
    panel: cocoa::base::id,
) -> Result<cocoa::base::id, String> {
    if panel.is_null() {
        Err("settings import picker is unavailable".to_string())
    } else {
        Ok(panel)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn classify_settings_import_panel_response(response: isize) -> Result<bool, String> {
    match response {
        1 => Ok(true),
        0 => Ok(false),
        _ => Err("settings import picker failed".to_string()),
    }
}

#[cfg(target_os = "macos")]
unsafe fn prepare_settings_import_panel(
    window: &tauri::WebviewWindow,
) -> Result<(cocoa::base::id, cocoa::base::id), String> {
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSArray, NSString};
    use objc::{class, msg_send, sel, sel_impl};

    let parent = window
        .ns_window()
        .map_err(|_| "failed to access the settings window".to_string())?
        .cast::<objc::runtime::Object>();
    if parent.is_null() {
        return Err("failed to access the settings window".to_string());
    }

    // Use the raw Objective-C initializer instead of objc2's non-null
    // `NSOpenPanel::openPanel` binding. AppKit can rarely return nil here;
    // checking the raw pointer keeps that condition recoverable instead of
    // aborting the process across the WebKit/Objective-C boundary.
    let panel: id = msg_send![class!(NSOpenPanel), new];
    let panel = require_settings_import_panel(panel)?;

    let extension = NSString::alloc(nil).init_str("json");
    if extension.is_null() {
        let _: () = msg_send![panel, release];
        return Err("failed to configure the settings import picker".to_string());
    }
    let allowed_types = NSArray::arrayWithObject(nil, extension);
    if allowed_types.is_null() {
        let _: () = msg_send![extension, release];
        let _: () = msg_send![panel, release];
        return Err("failed to configure the settings import picker".to_string());
    }

    let _: () = msg_send![panel, setAllowedFileTypes: allowed_types];
    let _: () = msg_send![panel, setCanChooseFiles: YES];
    let _: () = msg_send![panel, setCanChooseDirectories: NO];
    let _: () = msg_send![panel, setAllowsMultipleSelection: NO];
    let _: () = msg_send![panel, setResolvesAliases: YES];
    let _: () = msg_send![extension, release];

    Ok((panel, parent))
}

#[cfg(target_os = "macos")]
unsafe fn settings_import_path_from_panel(
    panel: cocoa::base::id,
    response: isize,
) -> Result<Option<PathBuf>, String> {
    use cocoa::base::id;
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    if !classify_settings_import_panel_response(response)? {
        return Ok(None);
    }

    let url: id = msg_send![panel, URL];
    if url.is_null() {
        return Err("settings import picker returned no file".to_string());
    }
    let representation: *const std::ffi::c_char = msg_send![url, fileSystemRepresentation];
    if representation.is_null() {
        return Err("unsupported settings import source".to_string());
    }

    let bytes = CStr::from_ptr(representation).to_bytes();
    Ok(Some(PathBuf::from(OsStr::from_bytes(bytes))))
}

#[cfg(target_os = "macos")]
async fn pick_settings_import_path(
    app: &AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<PathBuf>, String> {
    use block::ConcreteBlock;
    use objc::{msg_send, sel, sel_impl};

    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let prepared = unsafe { prepare_settings_import_panel(&window) };
        match prepared {
            Ok((panel, parent)) => {
                let sender = std::sync::Mutex::new(Some(sender));
                let completion = ConcreteBlock::new(move |response: isize| {
                    let sender = sender.lock().ok().and_then(|mut sender| sender.take());
                    let Some(sender) = sender else {
                        return;
                    };
                    let result = unsafe { settings_import_path_from_panel(panel, response) };
                    unsafe {
                        let _: () = msg_send![panel, orderOut: cocoa::base::nil];
                        let _: () = msg_send![panel, release];
                    }
                    let _ = sender.send(result);
                })
                .copy();

                unsafe {
                    let _: () = msg_send![
                        panel,
                        beginSheetModalForWindow: parent
                        completionHandler: &*completion
                    ];
                }
            }
            Err(error) => {
                let _ = sender.send(Err(error));
            }
        }
    })
    .map_err(|_| "failed to open the settings import picker".to_string())?;

    receiver
        .await
        .map_err(|_| "settings import picker closed unexpectedly".to_string())?
}

#[cfg(not(target_os = "macos"))]
async fn pick_settings_import_path(
    _app: &AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
            .map(|file_path| {
                file_path
                    .into_path()
                    .map_err(|_| "unsupported settings import source".to_string())
            })
            .transpose()
    })
    .await
    .map_err(|_| "failed to open the settings import picker".to_string())?
}

pub(super) fn decode_settings_import(
    document: &str,
    current: &AppSettings,
) -> Result<AppSettings, String> {
    if document.len() > MAX_SETTINGS_IMPORT_BYTES {
        return Err("settings backup exceeds the 2 MiB limit".to_string());
    }

    let value: serde_json::Value =
        serde_json::from_str(document).map_err(|_| "invalid settings backup JSON".to_string())?;

    if looks_like_settings_import_envelope(&value) {
        decode_legacy_settings_import(value, current)
    } else {
        decode_raw_settings_import(value)
    }
}

fn looks_like_settings_import_envelope(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|object| {
        SETTINGS_IMPORT_ENVELOPE_FIELDS
            .iter()
            .any(|field| object.contains_key(*field))
    })
}

fn decode_raw_settings_import(value: serde_json::Value) -> Result<AppSettings, String> {
    ensure_compatible_raw_settings(&value)?;
    let mut imported: AppSettings =
        serde_json::from_value(value).map_err(|_| "invalid settings backup data".to_string())?;
    validate_and_normalize_settings(&mut imported)?;
    Ok(imported)
}

fn decode_legacy_settings_import(
    value: serde_json::Value,
    current: &AppSettings,
) -> Result<AppSettings, String> {
    let envelope: SettingsImportEnvelope =
        serde_json::from_value(value).map_err(|_| "invalid settings backup data".to_string())?;

    if envelope.format != SETTINGS_EXPORT_FORMAT {
        return Err("unsupported settings backup format".to_string());
    }
    if envelope.schema_version != SETTINGS_EXPORT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported settings backup schema version: {}",
            envelope.schema_version
        ));
    }
    if envelope.app_version.trim().is_empty()
        || chrono::DateTime::parse_from_rfc3339(&envelope.exported_at).is_err()
    {
        return Err("invalid settings backup metadata".to_string());
    }
    if envelope.secrets_included || envelope.device_specific_fields_included {
        return Err("settings backup contains non-portable fields".to_string());
    }

    ensure_complete_settings(&envelope.settings)?;
    let mut imported: AppSettings = serde_json::from_value(envelope.settings)
        .map_err(|_| "invalid settings backup data".to_string())?;

    // Legacy V1 exports were redacted. Ignore any non-portable values embedded
    // in those envelopes and retain the target device's existing values.
    redact_nonportable_fields(&mut imported);
    validate_and_normalize_settings(&mut imported)?;
    restore_nonportable_fields(&mut imported, current);

    Ok(imported)
}

pub(super) fn write_settings_atomically(path: &Path, document: &str) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|_| "failed to create the settings directory".to_string())?;
    }

    let importing_path = path_with_suffix(path, ".importing");
    let previous_path = path_with_suffix(path, ".previous");
    recover_interrupted_settings_write(path)?;

    let existing_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut importing_file = options
        .open(&importing_path)
        .map_err(|_| "failed to create a temporary settings file".to_string())?;
    if let Some(permissions) = existing_permissions {
        if fs::set_permissions(&importing_path, permissions).is_err() {
            let _ = fs::remove_file(&importing_path);
            return Err("failed to preserve settings file permissions".to_string());
        }
    }
    if importing_file.write_all(document.as_bytes()).is_err() || importing_file.sync_all().is_err()
    {
        let _ = fs::remove_file(&importing_path);
        return Err("failed to write the temporary settings file".to_string());
    }
    drop(importing_file);

    let had_previous = path.exists();
    if had_previous && fs::rename(path, &previous_path).is_err() {
        let _ = fs::remove_file(&importing_path);
        return Err("failed to prepare the current settings file".to_string());
    }

    if fs::rename(&importing_path, path).is_err() {
        if had_previous {
            let _ = fs::rename(&previous_path, path);
        }
        let _ = fs::remove_file(&importing_path);
        return Err("failed to replace the settings file".to_string());
    }

    // The new file is already committed. Directory durability and cleanup are
    // best-effort so callers do not leave memory and disk out of sync after a
    // successful replacement.
    if let Err(error) = sync_parent_directory(path) {
        tracing::warn!(error = %error, "settings_directory_sync_failed");
    }
    if let Err(error) = remove_file_if_present(&previous_path) {
        tracing::warn!(error = %error, "previous_settings_cleanup_failed");
    }
    if let Err(error) = sync_parent_directory(path) {
        tracing::warn!(error = %error, "settings_cleanup_sync_failed");
    }

    Ok(())
}

pub(super) fn recover_interrupted_settings_write(path: &Path) -> Result<(), String> {
    let importing_path = path_with_suffix(path, ".importing");
    let previous_path = path_with_suffix(path, ".previous");
    let mut directory_changed = false;

    // If the main file is missing, the previous file is the last version known
    // to have been fully written and synced.
    if !path.exists() && previous_path.exists() {
        fs::rename(&previous_path, path)
            .map_err(|_| "failed to recover the previous settings file".to_string())?;
        directory_changed = true;
    }

    if importing_path.exists() {
        remove_file_if_present(&importing_path)?;
        directory_changed = true;
    }
    if path.exists() && previous_path.exists() {
        remove_file_if_present(&previous_path)?;
        directory_changed = true;
    }
    if directory_changed {
        sync_parent_directory(path)?;
    }

    Ok(())
}

#[tauri::command]
pub async fn export_settings(app: AppHandle) -> Result<SettingsExportResult, String> {
    let source_path = get_settings_path();
    let downloads_directory = app
        .path()
        .download_dir()
        .map_err(|_| "failed to locate the downloads directory".to_string())?;
    let export_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = export_app.state::<AppState>();
        let _settings_guard = state.settings.lock();
        export_settings_file_to_directory(&source_path, &downloads_directory)
    })
    .await
    .map_err(|_| "failed to export the settings file".to_string())??;

    tracing::info!("settings_exported");

    Ok(result)
}

#[tauri::command]
pub async fn select_settings_import(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<SettingsImportSelectionResult, String> {
    if window.label() != "main" {
        return Err("settings import is only available from the main window".to_string());
    }
    let source = pick_settings_import_path(&app, window).await?;
    tauri::async_runtime::spawn_blocking(move || {
        select_settings_import_with_source(|| Ok(source), read_settings_import_document)
    })
    .await
    .map_err(|_| "failed to read the settings backup".to_string())?
}

#[tauri::command]
pub fn import_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    document: String,
) -> Result<SettingsImportResult, String> {
    let imported = {
        let mut current = state.settings.lock();
        let imported = decode_settings_import(&document, &current)?;
        let mut settings_json = serde_json::to_string_pretty(&imported)
            .map_err(|_| "failed to serialize imported settings".to_string())?;
        settings_json.push('\n');
        write_settings_atomically(&get_settings_path(), &settings_json)?;
        *current = imported.clone();
        imported
    };

    if let Err(error) = app.emit(EventName::SETTINGS_CHANGED, imported) {
        tracing::warn!(error = %error, "settings_import_event_failed");
    }
    tracing::info!("settings_imported");

    Ok(SettingsImportResult {
        restart_required: true,
        restart_strategy: settings_import_restart_strategy(tauri::is_dev()),
    })
}

fn redact_nonportable_fields(settings: &mut AppSettings) {
    settings.audio_device = "default".to_string();
    settings.custom_dictionary.clear();

    for config in settings.cloud_stt_configs.values_mut() {
        config.api_key.clear();
        config.app_id.clear();
        config.base_url.clear();
    }
    for config in settings.cloud_polish_configs.values_mut() {
        config.api_key.clear();
        config.base_url.clear();
    }

    settings.local_polish_runtime.api_key.clear();
    settings.local_polish_runtime.base_url.clear();
    settings.local_polish_runtime.server_command.clear();
    settings.local_polish_runtime.server_args_json.clear();
}

fn restore_nonportable_fields(imported: &mut AppSettings, current: &AppSettings) {
    imported.audio_device.clone_from(&current.audio_device);
    imported
        .custom_dictionary
        .clone_from(&current.custom_dictionary);
    imported
        .local_polish_runtime
        .api_key
        .clone_from(&current.local_polish_runtime.api_key);
    imported
        .local_polish_runtime
        .base_url
        .clone_from(&current.local_polish_runtime.base_url);
    imported
        .local_polish_runtime
        .server_command
        .clone_from(&current.local_polish_runtime.server_command);
    imported
        .local_polish_runtime
        .server_args_json
        .clone_from(&current.local_polish_runtime.server_args_json);

    for (provider, current_config) in &current.cloud_stt_configs {
        if let Some(imported_config) = imported.cloud_stt_configs.get_mut(provider) {
            imported_config.api_key.clone_from(&current_config.api_key);
            imported_config.app_id.clone_from(&current_config.app_id);
            imported_config
                .base_url
                .clone_from(&current_config.base_url);
        } else {
            imported
                .cloud_stt_configs
                .insert(provider.clone(), current_config.clone());
        }
    }

    for (provider, current_config) in &current.cloud_polish_configs {
        if let Some(imported_config) = imported.cloud_polish_configs.get_mut(provider) {
            imported_config.api_key.clone_from(&current_config.api_key);
            imported_config
                .base_url
                .clone_from(&current_config.base_url);
        } else {
            imported
                .cloud_polish_configs
                .insert(provider.clone(), current_config.clone());
        }
    }
}

fn validate_and_normalize_settings(settings: &mut AppSettings) -> Result<(), String> {
    validate_choice(
        "theme mode",
        &settings.theme_mode,
        &["system", "light", "dark"],
    )?;
    validate_choice(
        "recording mode",
        &settings.recording_mode,
        &["hold", "toggle"],
    )?;
    validate_choice(
        "pill position",
        &settings.pill_position,
        &[
            "top-left",
            "top-center",
            "top-right",
            "bottom-left",
            "bottom-center",
            "bottom-right",
        ],
    )?;
    validate_choice(
        "pill indicator mode",
        &settings.pill_indicator_mode,
        &["always", "when_recording", "never"],
    )?;
    validate_choice(
        "denoise mode",
        &settings.denoise_mode,
        &["auto", "on", "off"],
    )?;

    if !(1..=5).contains(&settings.pill_size) {
        return Err("invalid pill size in settings backup".to_string());
    }
    if !settings.pill_background_opacity.is_finite()
        || !(0.2..=1.0).contains(&settings.pill_background_opacity)
    {
        return Err("invalid pill background opacity in settings backup".to_string());
    }
    settings.pill_background_color =
        normalize_pill_background_color(&settings.pill_background_color)
            .ok_or_else(|| "invalid pill background color in settings backup".to_string())?;

    validate_shortcut_profiles(settings)?;
    validate_custom_templates(settings)?;
    validate_provider_configs(settings)?;

    Ok(())
}

fn ensure_compatible_raw_settings(value: &serde_json::Value) -> Result<(), String> {
    let imported = ensure_object_fields_with_optional_missing(
        value,
        V1_SETTINGS_FIELDS,
        RAW_SETTINGS_OPTIONAL_FIELDS,
    )?;
    ensure_complete_nested_settings(imported)
}

fn ensure_complete_settings(value: &serde_json::Value) -> Result<(), String> {
    let imported = ensure_object_fields(value, V1_SETTINGS_FIELDS, &[])?;
    ensure_complete_nested_settings(imported)
}

fn ensure_complete_nested_settings(
    imported: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let shortcut_profiles = ensure_object_fields(
        imported
            .get("shortcut_profiles")
            .ok_or_else(invalid_backup_data)?,
        &["dictate", "riff"],
        &["custom"],
    )?;
    for profile_name in ["dictate", "riff", "custom"] {
        let Some(profile) = shortcut_profiles.get(profile_name) else {
            continue;
        };
        let profile = ensure_object_fields(profile, V1_SHORTCUT_PROFILE_FIELDS, &[])?;
        let action = ensure_object_fields(
            profile.get("action").ok_or_else(invalid_backup_data)?,
            &["Record"],
            &[],
        )?;
        ensure_object_fields(
            action.get("Record").ok_or_else(invalid_backup_data)?,
            &["polish_template_id"],
            &[],
        )?;
    }

    ensure_config_map(
        imported
            .get("cloud_stt_configs")
            .ok_or_else(invalid_backup_data)?,
        V1_CLOUD_STT_CONFIG_FIELDS,
    )?;
    ensure_config_map(
        imported
            .get("cloud_polish_configs")
            .ok_or_else(invalid_backup_data)?,
        V1_CLOUD_POLISH_CONFIG_FIELDS,
    )?;
    ensure_object_fields(
        imported
            .get("local_polish_runtime")
            .ok_or_else(invalid_backup_data)?,
        V1_LOCAL_RUNTIME_FIELDS,
        &[],
    )?;

    let templates = imported
        .get("polish_custom_templates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(invalid_backup_data)?;
    for template in templates {
        ensure_object_fields(template, V1_CUSTOM_TEMPLATE_FIELDS, &[])?;
    }

    Ok(())
}

fn ensure_config_map(value: &serde_json::Value, fields: &[&str]) -> Result<(), String> {
    let configs = value.as_object().ok_or_else(invalid_backup_data)?;
    for config in configs.values() {
        ensure_object_fields(config, fields, &[])?;
    }
    Ok(())
}

fn ensure_object_fields_with_optional_missing<'a>(
    value: &'a serde_json::Value,
    known: &[&str],
    optional: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
    let object = value.as_object().ok_or_else(invalid_backup_data)?;
    let missing_required = known
        .iter()
        .filter(|field| !optional.contains(field))
        .any(|field| !object.contains_key(*field));
    let has_unknown = object.keys().any(|field| !known.contains(&field.as_str()));
    if missing_required || has_unknown {
        return Err("settings backup does not match the raw settings schema".to_string());
    }
    Ok(object)
}

fn ensure_object_fields<'a>(
    value: &'a serde_json::Value,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
    let object = value.as_object().ok_or_else(invalid_backup_data)?;
    let missing_required = required.iter().any(|field| !object.contains_key(*field));
    let has_unknown = object
        .keys()
        .any(|field| !required.contains(&field.as_str()) && !optional.contains(&field.as_str()));

    if missing_required || has_unknown {
        return Err("settings backup does not match the V1 schema".to_string());
    }

    Ok(object)
}

fn invalid_backup_data() -> String {
    "invalid settings backup data".to_string()
}

fn validate_choice(name: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("invalid {name} in settings backup"))
    }
}

fn validate_shortcut_profiles(settings: &mut AppSettings) -> Result<(), String> {
    let mut hotkeys = HashSet::new();
    validate_shortcut_profile(
        "dictate",
        &mut settings.shortcut_profiles.dictate,
        &mut hotkeys,
    )?;
    validate_shortcut_profile("riff", &mut settings.shortcut_profiles.riff, &mut hotkeys)?;
    if let Some(custom) = settings.shortcut_profiles.custom.as_mut() {
        validate_shortcut_profile("custom", custom, &mut hotkeys)?;
    }

    match &settings.shortcut_profiles.dictate.action {
        ShortcutAction::Record {
            polish_template_id: None,
        } => {}
        _ => return Err("dictate shortcut cannot use a polish template".to_string()),
    }
    match &settings.shortcut_profiles.riff.action {
        ShortcutAction::Record {
            polish_template_id: Some(template_id),
        } if !template_id.trim().is_empty() => {}
        _ => return Err("riff shortcut requires a polish template".to_string()),
    }

    Ok(())
}

fn validate_shortcut_profile(
    name: &str,
    profile: &mut ShortcutProfile,
    hotkeys: &mut HashSet<String>,
) -> Result<(), String> {
    if profile.hotkey.is_empty() {
        return Ok(());
    }
    let canonical = canonicalize_hotkey_string(&profile.hotkey)
        .map_err(|_| format!("invalid {name} shortcut in settings backup"))?;
    if !hotkeys.insert(canonical.clone()) {
        return Err("duplicate shortcuts in settings backup".to_string());
    }
    profile.hotkey = canonical;
    Ok(())
}

fn validate_custom_templates(settings: &AppSettings) -> Result<(), String> {
    let mut template_ids = HashSet::new();
    for template in &settings.polish_custom_templates {
        if template.id.trim().is_empty() || !template_ids.insert(template.id.as_str()) {
            return Err("invalid custom polish templates in settings backup".to_string());
        }
    }
    Ok(())
}

fn validate_provider_configs(settings: &AppSettings) -> Result<(), String> {
    if settings
        .cloud_stt_configs
        .iter()
        .any(|(provider, config)| provider != &config.provider_type)
    {
        return Err("invalid cloud STT provider settings in backup".to_string());
    }
    if settings
        .cloud_polish_configs
        .iter()
        .any(|(provider, config)| provider != &config.provider_type)
    {
        return Err("invalid cloud polish provider settings in backup".to_string());
    }
    if settings.cloud_stt_enabled
        && !settings
            .cloud_stt_configs
            .contains_key(&settings.active_cloud_stt_provider)
    {
        return Err("active cloud STT provider is missing from backup".to_string());
    }
    if settings.cloud_polish_enabled
        && !settings
            .cloud_polish_configs
            .contains_key(&settings.active_cloud_polish_provider)
    {
        return Err("active cloud polish provider is missing from backup".to_string());
    }

    Ok(())
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    PathBuf::from(value)
}

pub(super) fn previous_settings_path(path: &Path) -> PathBuf {
    path_with_suffix(path, ".previous")
}

fn remove_file_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("failed to clean up a temporary settings file".to_string()),
    }
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let directory = File::open(parent)
            .map_err(|_| "failed to open the settings directory for sync".to_string())?;
        directory
            .sync_all()
            .map_err(|_| "failed to sync the settings directory".to_string())?;
    }

    #[cfg(not(unix))]
    let _ = path;

    Ok(())
}
