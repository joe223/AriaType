use super::{
    classify_cloud_check_error, migrate_platform_shortcut_defaults_for_test,
    migrate_to_profiles_map_for_test, normalize_pill_background_color,
    normalize_pill_background_opacity, polish_runtime_action_for_setting_update,
    validate_cloud_polish_config_for_check, validate_cloud_stt_config_for_check, AppSettings,
    CloudProviderConfig, CloudSttConfig, LocalPolishRuntimeSettingAction,
    LocalPolishRuntimeSettings,
};
use serde_json::json;
use std::fs;

use super::transfer::{
    decode_settings_import, export_settings_file_to_directory, read_settings_import_document,
    recover_interrupted_settings_write, select_settings_import_with_source,
    settings_import_restart_strategy, write_settings_atomically, SettingsExportResult,
    SettingsImportRestartStrategy, SettingsImportResult, SettingsImportSelectionResult,
    MAX_SETTINGS_IMPORT_BYTES,
};

#[cfg(target_os = "macos")]
use super::transfer::{classify_settings_import_panel_response, require_settings_import_panel};

fn settings_with_sensitive_values() -> AppSettings {
    let mut settings = AppSettings {
        theme_mode: "dark".to_string(),
        audio_device: "Studio Microphone".to_string(),
        stt_engine_user_glossary: "AriaType, Tauri".to_string(),
        custom_dictionary: "private -> phrase".to_string(),
        ..AppSettings::default()
    };
    settings.cloud_stt_configs.insert(
        "volcengine-streaming".to_string(),
        CloudSttConfig {
            enabled: true,
            provider_type: "volcengine-streaming".to_string(),
            api_key: "source-stt-secret".to_string(),
            app_id: "source-app-id".to_string(),
            base_url: "https://example.com/stt".to_string(),
            model: "bigmodel".to_string(),
            language: "auto".to_string(),
        },
    );
    settings.cloud_polish_configs.insert(
        "openai".to_string(),
        CloudProviderConfig {
            enabled: true,
            provider_type: "openai".to_string(),
            api_key: "source-polish-secret".to_string(),
            base_url: "https://example.com/v1".to_string(),
            model: "example-model".to_string(),
            enable_thinking: false,
        },
    );
    settings.local_polish_runtime.api_key = "source-local-secret".to_string();
    settings.local_polish_runtime.base_url = "http://source-device:8000/v1".to_string();
    settings.local_polish_runtime.server_command = "/source/llama-server".to_string();
    settings.local_polish_runtime.server_args_json = "[\"--source\"]".to_string();
    settings
}

fn legacy_settings_export_document(settings: &AppSettings) -> String {
    json!({
        "format": "ariatype-settings",
        "schema_version": 1,
        "app_version": "1.0.5",
        "exported_at": "2026-07-12T00:00:00Z",
        "secrets_included": false,
        "device_specific_fields_included": false,
        "settings": settings,
    })
    .to_string()
}

#[test]
fn settings_export_copies_the_current_settings_file_byte_for_byte_to_downloads() {
    let directory = tempfile::tempdir().unwrap();
    let app_config_directory = directory.path().join("app-config");
    let downloads_directory = directory.path().join("Downloads");
    fs::create_dir_all(&app_config_directory).unwrap();
    fs::create_dir_all(&downloads_directory).unwrap();
    let source_path = app_config_directory.join("settings.json");
    let source_document =
        b"{\r\n  \"api_key\": \"source-secret\",  \r\n  \"custom\": \"value\"\r\n}\r\n";
    fs::write(&source_path, source_document).unwrap();

    let result = export_settings_file_to_directory(&source_path, &downloads_directory).unwrap();
    let destination_path = downloads_directory.join("ariatype-settings.json");

    assert_eq!(
        result,
        SettingsExportResult::Saved {
            file_path: destination_path.to_string_lossy().into_owned(),
        }
    );
    assert_eq!(fs::read(&destination_path).unwrap(), source_document);
    assert_eq!(fs::read(&source_path).unwrap(), source_document);
}

#[test]
fn settings_export_uses_a_unique_name_without_overwriting_an_existing_export() {
    let directory = tempfile::tempdir().unwrap();
    let downloads_directory = directory.path().join("Downloads");
    fs::create_dir_all(&downloads_directory).unwrap();
    let source_path = directory.path().join("settings.json");
    let existing_path = downloads_directory.join("ariatype-settings.json");
    let source_document = b"{\"source\":true}\n";
    let existing_document = b"{\"existing\":true}\n";
    fs::write(&source_path, source_document).unwrap();
    fs::write(&existing_path, existing_document).unwrap();

    let result = export_settings_file_to_directory(&source_path, &downloads_directory).unwrap();
    let destination_path = downloads_directory.join("ariatype-settings-2.json");

    assert_eq!(
        result,
        SettingsExportResult::Saved {
            file_path: destination_path.to_string_lossy().into_owned(),
        }
    );
    assert_eq!(fs::read(&existing_path).unwrap(), existing_document);
    assert_eq!(fs::read(&destination_path).unwrap(), source_document);
}

#[test]
fn settings_export_rejects_a_missing_settings_source() {
    let directory = tempfile::tempdir().unwrap();
    let downloads_directory = directory.path().join("Downloads");
    fs::create_dir_all(&downloads_directory).unwrap();

    let result = export_settings_file_to_directory(
        &directory.path().join("missing-settings.json"),
        &downloads_directory,
    );

    assert!(result.is_err());
    assert_eq!(fs::read_dir(&downloads_directory).unwrap().count(), 0);
}

#[test]
fn settings_export_rejects_a_non_file_settings_source() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("settings.json");
    let downloads_directory = directory.path().join("Downloads");
    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(&downloads_directory).unwrap();

    let result = export_settings_file_to_directory(&source_path, &downloads_directory);

    assert!(result.is_err());
    assert_eq!(fs::read_dir(&downloads_directory).unwrap().count(), 0);
}

#[test]
fn settings_export_result_serializes_the_ipc_contract() {
    let saved = SettingsExportResult::Saved {
        file_path: "/tmp/ariatype-settings.json".to_string(),
    };

    assert_eq!(
        serde_json::to_value(saved).unwrap(),
        json!({
            "status": "saved",
            "file_path": "/tmp/ariatype-settings.json"
        })
    );
}

#[test]
fn settings_import_selection_reads_the_selected_native_path() {
    let directory = tempfile::tempdir().unwrap();
    let selected_path = directory.path().join("portable-settings.json");
    let expected_path = selected_path.clone();
    let document = "sensitive settings document";

    let result = select_settings_import_with_source(
        || Ok(Some(selected_path)),
        |path| {
            assert_eq!(path, expected_path);
            Ok(document.to_string())
        },
    )
    .unwrap();

    assert_eq!(
        result,
        SettingsImportSelectionResult::Selected {
            document: document.to_string()
        }
    );
}

#[test]
fn settings_import_selection_cancellation_does_not_read_a_file() {
    let read_calls = std::cell::Cell::new(0);

    let result = select_settings_import_with_source(
        || Ok(None),
        |_: &std::path::Path| {
            read_calls.set(read_calls.get() + 1);
            Ok(String::new())
        },
    )
    .unwrap();

    assert_eq!(result, SettingsImportSelectionResult::Cancelled);
    assert_eq!(read_calls.get(), 0);
}

#[test]
fn settings_import_reader_accepts_exactly_two_mib() {
    let directory = tempfile::tempdir().unwrap();
    let selected_path = directory.path().join("settings.json");
    let document = "x".repeat(MAX_SETTINGS_IMPORT_BYTES);
    fs::write(&selected_path, &document).unwrap();

    assert_eq!(
        read_settings_import_document(&selected_path).unwrap(),
        document
    );
}

#[test]
fn settings_import_reader_rejects_more_than_two_mib() {
    let directory = tempfile::tempdir().unwrap();
    let selected_path = directory.path().join("settings.json");
    fs::write(&selected_path, vec![b'x'; MAX_SETTINGS_IMPORT_BYTES + 1]).unwrap();

    assert!(read_settings_import_document(&selected_path).is_err());
}

#[test]
fn settings_import_reader_rejects_invalid_utf8() {
    let directory = tempfile::tempdir().unwrap();
    let selected_path = directory.path().join("settings.json");
    fs::write(&selected_path, [0xff, 0xfe, 0xfd]).unwrap();

    assert!(read_settings_import_document(&selected_path).is_err());
}

#[cfg(unix)]
#[test]
fn settings_import_reader_rejects_a_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let directory = tempfile::tempdir().unwrap();
    let selected_path = directory.path().join("settings.json");
    let path = CString::new(selected_path.as_os_str().as_bytes()).unwrap();
    let status = unsafe { libc::mkfifo(path.as_ptr(), 0o600) };
    assert_eq!(status, 0);

    assert!(read_settings_import_document(&selected_path).is_err());
}

#[test]
fn settings_import_selection_result_serializes_the_ipc_contract() {
    assert_eq!(
        serde_json::to_value(SettingsImportSelectionResult::Selected {
            document: "sensitive settings document".to_string(),
        })
        .unwrap(),
        json!({
            "status": "selected",
            "document": "sensitive settings document"
        })
    );
    assert_eq!(
        serde_json::to_value(SettingsImportSelectionResult::Cancelled).unwrap(),
        json!({ "status": "cancelled" })
    );
}

#[test]
fn settings_import_restart_strategy_is_manual_in_development() {
    assert_eq!(
        settings_import_restart_strategy(true),
        SettingsImportRestartStrategy::Manual
    );
}

#[test]
fn settings_import_restart_strategy_is_automatic_in_release() {
    assert_eq!(
        settings_import_restart_strategy(false),
        SettingsImportRestartStrategy::Automatic
    );
}

#[test]
fn settings_import_result_serializes_the_ipc_contract() {
    assert_eq!(
        serde_json::to_value(SettingsImportResult {
            restart_required: true,
            restart_strategy: SettingsImportRestartStrategy::Manual,
        })
        .unwrap(),
        json!({
            "restart_required": true,
            "restart_strategy": "manual"
        })
    );
    assert_eq!(
        serde_json::to_value(SettingsImportResult {
            restart_required: true,
            restart_strategy: SettingsImportRestartStrategy::Automatic,
        })
        .unwrap(),
        json!({
            "restart_required": true,
            "restart_strategy": "automatic"
        })
    );
}

#[cfg(target_os = "macos")]
#[test]
fn settings_import_picker_treats_a_nil_panel_as_a_recoverable_error() {
    assert!(require_settings_import_panel(cocoa::base::nil).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn settings_import_picker_distinguishes_selection_cancellation_and_failure() {
    assert_eq!(classify_settings_import_panel_response(1).unwrap(), true);
    assert_eq!(classify_settings_import_panel_response(0).unwrap(), false);
    assert!(classify_settings_import_panel_response(-1001).is_err());
}

#[test]
fn raw_settings_import_preserves_source_credentials_endpoints_and_device_values() {
    let mut source = settings_with_sensitive_values();
    source.local_polish_runtime.ready_timeout_secs = 41;
    source.window_context_enabled = false;
    let source_document = serde_json::to_string_pretty(&source).unwrap();
    let mut target = settings_with_sensitive_values();
    target.theme_mode = "light".to_string();
    target.audio_device = "Target Microphone".to_string();
    target.stt_engine_user_glossary = "target glossary".to_string();
    target.custom_dictionary = "target -> phrase".to_string();
    target
        .cloud_stt_configs
        .get_mut("volcengine-streaming")
        .unwrap()
        .api_key = "target-stt-secret".to_string();
    target
        .cloud_stt_configs
        .get_mut("volcengine-streaming")
        .unwrap()
        .app_id = "target-app-id".to_string();
    target
        .cloud_stt_configs
        .get_mut("volcengine-streaming")
        .unwrap()
        .base_url = "https://target.example.com/stt".to_string();
    target
        .cloud_polish_configs
        .get_mut("openai")
        .unwrap()
        .api_key = "target-polish-secret".to_string();
    target
        .cloud_polish_configs
        .get_mut("openai")
        .unwrap()
        .base_url = "https://target.example.com/v1".to_string();
    target.cloud_stt_configs.insert(
        "target-only".to_string(),
        CloudSttConfig {
            enabled: false,
            provider_type: "target-only".to_string(),
            api_key: "target-only-stt-secret".to_string(),
            app_id: "target-only-app-id".to_string(),
            base_url: "https://target-only.example.com/stt".to_string(),
            model: "target-only-model".to_string(),
            language: "auto".to_string(),
        },
    );
    target.cloud_polish_configs.insert(
        "target-only".to_string(),
        CloudProviderConfig {
            enabled: false,
            provider_type: "target-only".to_string(),
            api_key: "target-only-secret".to_string(),
            base_url: "https://target-only.example.com/v1".to_string(),
            model: "target-only-model".to_string(),
            enable_thinking: false,
        },
    );
    target.local_polish_runtime.api_key = "target-local-secret".to_string();
    target.local_polish_runtime.base_url = "http://target-device:8000/v1".to_string();
    target.local_polish_runtime.server_command = "/target/llama-server".to_string();
    target.local_polish_runtime.server_args_json = "[\"--target\"]".to_string();
    target.local_polish_runtime.ready_timeout_secs = 99;

    let imported = decode_settings_import(&source_document, &target).unwrap();

    assert_eq!(imported.theme_mode, source.theme_mode);
    assert_eq!(imported.audio_device, "Studio Microphone");
    assert_eq!(imported.stt_engine_user_glossary, "AriaType, Tauri");
    assert_eq!(imported.custom_dictionary, "private -> phrase");
    assert_eq!(
        imported.cloud_stt_configs["volcengine-streaming"].api_key,
        "source-stt-secret"
    );
    assert_eq!(
        imported.cloud_stt_configs["volcengine-streaming"].app_id,
        "source-app-id"
    );
    assert_eq!(
        imported.cloud_stt_configs["volcengine-streaming"].base_url,
        "https://example.com/stt"
    );
    assert_eq!(
        imported.cloud_polish_configs["openai"].api_key,
        "source-polish-secret"
    );
    assert_eq!(
        imported.cloud_polish_configs["openai"].base_url,
        "https://example.com/v1"
    );
    assert!(!imported.cloud_stt_configs.contains_key("target-only"));
    assert!(!imported.cloud_polish_configs.contains_key("target-only"));
    assert_eq!(imported.local_polish_runtime.api_key, "source-local-secret");
    assert_eq!(
        imported.local_polish_runtime.base_url,
        "http://source-device:8000/v1"
    );
    assert_eq!(
        imported.local_polish_runtime.server_command,
        "/source/llama-server"
    );
    assert_eq!(
        imported.local_polish_runtime.server_args_json,
        "[\"--source\"]"
    );
    assert_eq!(imported.local_polish_runtime.ready_timeout_secs, 41);
    assert!(!imported.window_context_enabled);
}

#[test]
fn raw_settings_import_accepts_known_defaulted_fields() {
    let mut source = serde_json::to_value(AppSettings::default()).unwrap();
    let source_object = source.as_object_mut().unwrap();
    source_object.remove("correction_memory_enabled");
    source_object.remove("pill_size");
    source_object.remove("pill_background_color");
    source_object.remove("pill_background_opacity");
    source_object.remove("stay_in_tray");
    source_object.remove("window_context_enabled");

    let imported = decode_settings_import(&source.to_string(), &AppSettings::default()).unwrap();

    assert!(imported.correction_memory_enabled);
    assert_eq!(imported.pill_size, 2);
    assert_eq!(imported.pill_background_color, "#1d1d1d");
    assert_eq!(imported.pill_background_opacity, 1.0);
    assert!(imported.stay_in_tray);
    assert!(imported.window_context_enabled);
}

#[test]
fn raw_settings_import_rejects_accidental_partial_documents() {
    let current = AppSettings::default();
    let defaults = serde_json::to_value(AppSettings::default()).unwrap();
    let plausible_but_truncated = json!({
        "shortcut_profiles": defaults["shortcut_profiles"].clone(),
        "recording_mode": defaults["recording_mode"].clone(),
        "model": defaults["model"].clone(),
        "stt_engine": defaults["stt_engine"].clone(),
        "language": defaults["language"].clone(),
        "theme_mode": defaults["theme_mode"].clone(),
    });

    assert!(decode_settings_import("{}", &current).is_err());
    assert!(decode_settings_import(r#"{"theme_mode":"dark"}"#, &current).is_err());
    assert!(decode_settings_import(&plausible_but_truncated.to_string(), &current).is_err());
}

#[test]
fn raw_settings_import_rejects_unknown_fields_instead_of_dropping_them() {
    let current = AppSettings::default();
    let mut unknown_top_level = serde_json::to_value(AppSettings::default()).unwrap();
    unknown_top_level["future_setting"] = json!(true);
    let mut unknown_provider_field =
        serde_json::to_value(settings_with_sensitive_values()).unwrap();
    unknown_provider_field["cloud_polish_configs"]["openai"]["future_setting"] = json!(true);
    let mut incomplete_provider = serde_json::to_value(settings_with_sensitive_values()).unwrap();
    incomplete_provider["cloud_polish_configs"]["openai"] = json!({
        "provider_type": "openai"
    });

    assert!(decode_settings_import(&unknown_top_level.to_string(), &current).is_err());
    assert!(decode_settings_import(&unknown_provider_field.to_string(), &current).is_err());
    assert!(decode_settings_import(&incomplete_provider.to_string(), &current).is_err());
}

#[test]
fn legacy_envelope_import_restores_preferences_and_keeps_target_secrets() {
    let source = settings_with_sensitive_values();
    let document = legacy_settings_export_document(&source);
    let mut target = settings_with_sensitive_values();
    target.theme_mode = "light".to_string();
    target.audio_device = "Target Microphone".to_string();
    target.custom_dictionary = "target -> phrase".to_string();
    target
        .cloud_stt_configs
        .get_mut("volcengine-streaming")
        .unwrap()
        .api_key = "target-stt-secret".to_string();
    target
        .cloud_stt_configs
        .get_mut("volcengine-streaming")
        .unwrap()
        .app_id = "target-app-id".to_string();
    target
        .cloud_stt_configs
        .get_mut("volcengine-streaming")
        .unwrap()
        .base_url = "https://target.example.com/stt".to_string();
    target
        .cloud_polish_configs
        .get_mut("openai")
        .unwrap()
        .api_key = "target-polish-secret".to_string();
    target
        .cloud_polish_configs
        .get_mut("openai")
        .unwrap()
        .base_url = "https://target.example.com/v1".to_string();
    target.cloud_polish_configs.insert(
        "target-only".to_string(),
        CloudProviderConfig {
            enabled: false,
            provider_type: "target-only".to_string(),
            api_key: "target-only-secret".to_string(),
            base_url: "https://target-only.example.com/v1".to_string(),
            model: "target-only-model".to_string(),
            enable_thinking: false,
        },
    );
    target.local_polish_runtime.api_key = "target-local-secret".to_string();
    target.local_polish_runtime.base_url = "http://target-device:8000/v1".to_string();
    target.local_polish_runtime.server_command = "/target/llama-server".to_string();
    target.local_polish_runtime.server_args_json = "[\"--target\"]".to_string();

    let imported = decode_settings_import(&document, &target).unwrap();

    assert_eq!(imported.theme_mode, "dark");
    assert_eq!(imported.audio_device, "Target Microphone");
    assert_eq!(imported.custom_dictionary, "target -> phrase");
    assert_eq!(
        imported.cloud_stt_configs["volcengine-streaming"].api_key,
        "target-stt-secret"
    );
    assert_eq!(
        imported.cloud_stt_configs["volcengine-streaming"].app_id,
        "target-app-id"
    );
    assert_eq!(
        imported.cloud_stt_configs["volcengine-streaming"].base_url,
        "https://target.example.com/stt"
    );
    assert_eq!(
        imported.cloud_polish_configs["openai"].api_key,
        "target-polish-secret"
    );
    assert_eq!(
        imported.cloud_polish_configs["openai"].base_url,
        "https://target.example.com/v1"
    );
    assert_eq!(
        imported.cloud_polish_configs["target-only"].api_key,
        "target-only-secret"
    );
    assert_eq!(
        imported.cloud_polish_configs["target-only"].model,
        "target-only-model"
    );
    assert_eq!(imported.local_polish_runtime.api_key, "target-local-secret");
    assert_eq!(
        imported.local_polish_runtime.base_url,
        "http://target-device:8000/v1"
    );
    assert_eq!(
        imported.local_polish_runtime.server_command,
        "/target/llama-server"
    );
    assert_eq!(
        imported.local_polish_runtime.server_args_json,
        "[\"--target\"]"
    );
}

#[test]
fn legacy_envelope_import_ignores_credentials_embedded_in_document() {
    let mut document: serde_json::Value =
        serde_json::from_str(&legacy_settings_export_document(&AppSettings::default())).unwrap();
    document["settings"]["cloud_polish_configs"]["untrusted"] = json!({
        "enabled": true,
        "provider_type": "untrusted",
        "api_key": "embedded-secret",
        "base_url": "https://example.com/v1",
        "model": "example-model",
        "enable_thinking": false
    });
    document["settings"]["local_polish_runtime"]["server_command"] =
        json!("/tmp/untrusted-command");
    document["settings"]["local_polish_runtime"]["base_url"] =
        json!("http://untrusted-device:8000/v1");

    let imported = decode_settings_import(&document.to_string(), &AppSettings::default()).unwrap();

    assert_eq!(imported.cloud_polish_configs["untrusted"].api_key, "");
    assert_eq!(imported.cloud_polish_configs["untrusted"].base_url, "");
    assert_eq!(imported.local_polish_runtime.server_command, "");
    assert_eq!(
        imported.local_polish_runtime.base_url,
        LocalPolishRuntimeSettings::default().base_url
    );
}

#[test]
fn settings_import_rejects_invalid_format_version_and_size() {
    let legacy_document = legacy_settings_export_document(&AppSettings::default());
    let current = AppSettings::default();
    let mut wrong_format: serde_json::Value = serde_json::from_str(&legacy_document).unwrap();
    wrong_format["format"] = json!("other-app");
    let mut future_version: serde_json::Value = serde_json::from_str(&legacy_document).unwrap();
    future_version["schema_version"] = json!(2);
    let mut invalid_metadata: serde_json::Value = serde_json::from_str(&legacy_document).unwrap();
    invalid_metadata["exported_at"] = json!("not-a-timestamp");
    let mut secrets_included: serde_json::Value = serde_json::from_str(&legacy_document).unwrap();
    secrets_included["secrets_included"] = json!(true);
    let mut device_fields_included: serde_json::Value =
        serde_json::from_str(&legacy_document).unwrap();
    device_fields_included["device_specific_fields_included"] = json!(true);
    let mut incomplete: serde_json::Value = serde_json::from_str(&legacy_document).unwrap();
    incomplete["settings"]
        .as_object_mut()
        .unwrap()
        .remove("theme_mode");
    let mut unknown: serde_json::Value = serde_json::from_str(&legacy_document).unwrap();
    unknown["settings"]["future_setting"] = json!(true);
    let mut incomplete_provider: serde_json::Value =
        serde_json::from_str(&legacy_document).unwrap();
    incomplete_provider["settings"]["cloud_polish_configs"]["openai"] = json!({
        "provider_type": "openai"
    });

    assert!(decode_settings_import("not-json", &current).is_err());
    assert!(decode_settings_import(&wrong_format.to_string(), &current).is_err());
    assert!(decode_settings_import(&future_version.to_string(), &current).is_err());
    assert!(decode_settings_import(&invalid_metadata.to_string(), &current).is_err());
    assert!(decode_settings_import(&secrets_included.to_string(), &current).is_err());
    assert!(decode_settings_import(&device_fields_included.to_string(), &current).is_err());
    assert!(decode_settings_import(&incomplete.to_string(), &current).is_err());
    assert!(decode_settings_import(&unknown.to_string(), &current).is_err());
    assert!(decode_settings_import(&incomplete_provider.to_string(), &current).is_err());
    assert!(decode_settings_import(&"x".repeat(MAX_SETTINGS_IMPORT_BYTES + 1), &current,).is_err());
}

#[test]
fn settings_import_canonicalizes_shortcuts_and_allows_disabled_profiles() {
    let mut settings = AppSettings::default();
    settings.shortcut_profiles.dictate.hotkey = "command+dot".to_string();
    settings.shortcut_profiles.riff.hotkey.clear();
    let document = serde_json::to_string(&settings).unwrap();

    let imported = decode_settings_import(&document, &AppSettings::default()).unwrap();

    assert_eq!(imported.shortcut_profiles.dictate.hotkey, "Cmd+Dot");
    assert_eq!(imported.shortcut_profiles.riff.hotkey, "");
}

#[test]
fn settings_import_rejects_invalid_theme_and_duplicate_hotkeys() {
    let current = AppSettings::default();
    let mut invalid_theme = serde_json::to_value(AppSettings::default()).unwrap();
    invalid_theme["theme_mode"] = json!("neon");

    let mut duplicate_hotkeys = AppSettings::default();
    duplicate_hotkeys.shortcut_profiles.riff.hotkey =
        duplicate_hotkeys.shortcut_profiles.dictate.hotkey.clone();
    let duplicate_document = serde_json::to_string(&duplicate_hotkeys).unwrap();
    let mut mismatched_provider = serde_json::to_value(settings_with_sensitive_values()).unwrap();
    mismatched_provider["cloud_polish_configs"]["openai"]["provider_type"] = json!("other");

    assert!(decode_settings_import(&invalid_theme.to_string(), &current).is_err());
    assert!(decode_settings_import(&duplicate_document, &current).is_err());
    assert!(decode_settings_import(&mismatched_provider.to_string(), &current).is_err());
}

#[test]
fn settings_import_rejects_invalid_profile_actions_and_pill_values() {
    let current = AppSettings::default();
    let original = serde_json::to_value(AppSettings::default()).unwrap();

    let mut invalid_dictate = original.clone();
    invalid_dictate["shortcut_profiles"]["dictate"]["action"]["Record"]["polish_template_id"] =
        json!("filler");
    let mut invalid_riff = original.clone();
    invalid_riff["shortcut_profiles"]["riff"]["action"]["Record"]["polish_template_id"] =
        serde_json::Value::Null;
    let mut invalid_size = original.clone();
    invalid_size["pill_size"] = json!(6);
    let mut invalid_opacity = original;
    invalid_opacity["pill_background_opacity"] = json!(0.1);

    assert!(decode_settings_import(&invalid_dictate.to_string(), &current).is_err());
    assert!(decode_settings_import(&invalid_riff.to_string(), &current).is_err());
    assert!(decode_settings_import(&invalid_size.to_string(), &current).is_err());
    assert!(decode_settings_import(&invalid_opacity.to_string(), &current).is_err());
}

#[test]
fn settings_atomic_write_replaces_existing_document_without_residue() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, "old settings").unwrap();

    write_settings_atomically(&path, "new settings").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "new settings");
    assert!(!directory.path().join("settings.json.importing").exists());
    assert!(!directory.path().join("settings.json.previous").exists());
}

#[test]
fn settings_atomic_write_recovers_an_interrupted_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let previous_path = directory.path().join("settings.json.previous");
    let importing_path = directory.path().join("settings.json.importing");
    fs::write(&previous_path, "known good settings").unwrap();
    fs::write(&importing_path, "incomplete settings").unwrap();

    recover_interrupted_settings_write(&path).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "known good settings");
    assert!(!previous_path.exists());
    assert!(!importing_path.exists());
}

#[test]
fn test_is_streaming_stt_active_accepts_aliyun_stream_provider_id() {
    let mut settings = AppSettings::default();
    settings.cloud_stt_enabled = true;
    settings.active_cloud_stt_provider = "aliyun-stream".to_string();

    assert!(settings.is_streaming_stt_active());
}

#[test]
fn cloud_stt_check_validation_requires_schema_fields() {
    let mut config = CloudSttConfig {
        provider_type: "volcengine-streaming".to_string(),
        api_key: "token".to_string(),
        app_id: String::new(),
        base_url: String::new(),
        model: String::new(),
        language: String::new(),
        enabled: true,
    };

    let err = validate_cloud_stt_config_for_check(&config).unwrap_err();
    assert_eq!(err.kind, "missing_required");
    assert!(err.message.contains("App ID"));

    config.app_id = "app-id".to_string();
    assert!(validate_cloud_stt_config_for_check(&config).is_ok());
}

#[test]
fn cloud_polish_check_validation_requires_model() {
    let config = CloudProviderConfig {
        provider_type: "openai".to_string(),
        api_key: "sk-test".to_string(),
        base_url: String::new(),
        model: String::new(),
        enable_thinking: false,
        enabled: true,
    };

    let err = validate_cloud_polish_config_for_check(&config).unwrap_err();
    assert_eq!(err.kind, "missing_required");
    assert!(err.message.contains("Model"));
}

#[test]
fn cloud_check_validation_rejects_invalid_base_url() {
    let config = CloudProviderConfig {
        provider_type: "anthropic".to_string(),
        api_key: "sk-test".to_string(),
        base_url: "not a url".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        enable_thinking: false,
        enabled: true,
    };

    let err = validate_cloud_polish_config_for_check(&config).unwrap_err();
    assert_eq!(err.kind, "invalid_url");
}

#[test]
fn cloud_check_error_classifier_maps_auth_and_timeout() {
    assert_eq!(
        classify_cloud_check_error("API error (401 Unauthorized): invalid_api_key"),
        "auth_failed"
    );
    assert_eq!(
        classify_cloud_check_error("connection check timed out after 10s"),
        "timeout"
    );
    assert_eq!(
        classify_cloud_check_error("API error (404): model not found"),
        "model_failed"
    );
}

#[test]
fn enabling_cloud_polish_stops_managed_local_runtime() {
    assert_eq!(
        polish_runtime_action_for_setting_update("cloud_polish_enabled", &json!(true)),
        LocalPolishRuntimeSettingAction::StopManagedRuntime
    );
}

#[test]
fn disabling_cloud_polish_keeps_local_runtime_available() {
    assert_eq!(
        polish_runtime_action_for_setting_update("cloud_polish_enabled", &json!(false)),
        LocalPolishRuntimeSettingAction::None
    );
}

#[test]
fn migrate_from_legacy_hotkey_copies_global_recording_mode_into_profiles() {
    let mut json = json!({
        "hotkey": "Shift+Space",
        "recording_mode": "toggle",
    });

    migrate_to_profiles_map_for_test(&mut json);

    assert_eq!(
        json["shortcut_profiles"]["dictate"]["trigger_mode"],
        "toggle"
    );
    assert_eq!(json["shortcut_profiles"]["riff"]["trigger_mode"], "toggle");
}

#[test]
fn migrate_array_profiles_copies_global_recording_mode_into_existing_profiles() {
    let mut json = json!({
        "recording_mode": "hold",
        "shortcut_profiles": [
            {
                "hotkey": "Cmd+Slash",
                "action": { "Record": { "polish_template_id": null } }
            },
            {
                "hotkey": "Opt+Slash",
                "action": { "Record": { "polish_template_id": "filler" } }
            },
            {
                "hotkey": "Cmd+Alt+Slash",
                "action": { "Record": { "polish_template_id": "formal" } }
            }
        ]
    });

    migrate_to_profiles_map_for_test(&mut json);

    assert_eq!(json["shortcut_profiles"]["dictate"]["trigger_mode"], "hold");
    assert_eq!(json["shortcut_profiles"]["riff"]["trigger_mode"], "hold");
    assert_eq!(json["shortcut_profiles"]["custom"]["trigger_mode"], "hold");
}

#[test]
fn migrate_platform_shortcut_defaults_rewrites_legacy_defaults_on_windows() {
    let mut json = json!({
        "shortcut_profiles": {
            "dictate": {
                "hotkey": "Cmd+Slash",
                "trigger_mode": "hold",
                "action": { "Record": { "polish_template_id": null } }
            },
            "riff": {
                "hotkey": "Opt+Slash",
                "trigger_mode": "toggle",
                "action": { "Record": { "polish_template_id": "filler" } }
            },
            "custom": {
                "hotkey": "Cmd+Alt+Slash",
                "trigger_mode": "toggle",
                "action": { "Record": { "polish_template_id": null } }
            }
        }
    });

    let migrated = migrate_platform_shortcut_defaults_for_test(&mut json, false);

    assert!(migrated);
    assert_eq!(json["shortcut_profiles"]["dictate"]["hotkey"], "Ctrl+Dot");
    assert_eq!(json["shortcut_profiles"]["riff"]["hotkey"], "Alt+Slash");
    assert_eq!(
        json["shortcut_profiles"]["custom"]["hotkey"],
        "Cmd+Alt+Slash"
    );
}

#[test]
fn migrate_platform_shortcut_defaults_rewrites_legacy_default_on_macos() {
    let mut mac_json = json!({
        "shortcut_profiles": {
            "dictate": { "hotkey": "Cmd+Slash" },
            "riff": { "hotkey": "Opt+Slash" }
        }
    });

    assert!(migrate_platform_shortcut_defaults_for_test(
        &mut mac_json,
        true
    ));
    assert_eq!(
        mac_json["shortcut_profiles"]["dictate"]["hotkey"],
        "Cmd+Dot"
    );
    assert_eq!(mac_json["shortcut_profiles"]["riff"]["hotkey"], "Opt+Slash");
}

#[test]
fn migrate_platform_shortcut_defaults_keeps_customized_values() {
    let mut customized_json = json!({
        "shortcut_profiles": {
            "dictate": { "hotkey": "Shift+Space" },
            "riff": { "hotkey": "Ctrl+Space" }
        }
    });

    assert!(!migrate_platform_shortcut_defaults_for_test(
        &mut customized_json,
        false
    ));
    assert_eq!(
        customized_json["shortcut_profiles"]["dictate"]["hotkey"],
        "Shift+Space"
    );
    assert_eq!(
        customized_json["shortcut_profiles"]["riff"]["hotkey"],
        "Ctrl+Space"
    );
}

#[test]
fn migrate_platform_shortcut_defaults_rewrites_previous_windows_default() {
    let mut json = json!({
        "shortcut_profiles": {
            "dictate": { "hotkey": "Ctrl+Slash" },
            "riff": { "hotkey": "Alt+Slash" }
        }
    });

    assert!(migrate_platform_shortcut_defaults_for_test(
        &mut json, false
    ));
    assert_eq!(json["shortcut_profiles"]["dictate"]["hotkey"], "Ctrl+Dot");
    assert_eq!(json["shortcut_profiles"]["riff"]["hotkey"], "Alt+Slash");
}

#[test]
fn missing_pill_background_color_uses_default() {
    let settings: AppSettings = serde_json::from_value(json!({})).unwrap();

    assert_eq!(settings.pill_background_color, "#1d1d1d");
    assert_eq!(settings.pill_background_opacity, 1.0);
}

#[test]
fn correction_memory_defaults_enabled() {
    let settings: AppSettings = serde_json::from_value(json!({})).unwrap();

    assert!(settings.correction_memory_enabled);
}

#[test]
fn stay_in_tray_defaults_enabled() {
    let settings: AppSettings = serde_json::from_value(json!({})).unwrap();

    assert!(settings.stay_in_tray);
}

#[test]
fn local_polish_runtime_uses_default_when_missing() {
    let settings: AppSettings = serde_json::from_value(json!({})).unwrap();

    assert_eq!(settings.local_polish_runtime.provider_type, "llama-server");
    assert_eq!(
        settings.local_polish_runtime.base_url,
        "http://127.0.0.1:8000/v1"
    );
    assert_eq!(settings.local_polish_runtime.ready_timeout_secs, 20);
}

#[test]
fn direct_stream_typing_defaults_disabled() {
    let settings: AppSettings = serde_json::from_value(json!({})).unwrap();

    assert!(!settings.polish_stream_direct_typing_enabled);
}

#[test]
fn normalize_pill_background_color_accepts_only_hex_rgb_values() {
    assert_eq!(
        normalize_pill_background_color(" #AABBCC "),
        Some("#aabbcc".to_string())
    );
    assert_eq!(normalize_pill_background_color("#abc"), None);
    assert_eq!(normalize_pill_background_color("red"), None);
    assert_eq!(normalize_pill_background_color("#zzzzzz"), None);
}

#[test]
fn normalize_pill_background_opacity_clamps_to_visible_range() {
    assert_eq!(normalize_pill_background_opacity(0.65), Some(0.65));
    assert_eq!(normalize_pill_background_opacity(0.0), Some(0.2));
    assert_eq!(normalize_pill_background_opacity(1.5), Some(1.0));
    assert_eq!(normalize_pill_background_opacity(f64::NAN), None);
}
