use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tracing::instrument;

/// Shared helper used by both the `insert_text` command and the audio pipeline.
#[instrument(skip(app), fields(text_len = text.len()), err)]
pub fn do_insert_text(app: &AppHandle, text: &str) -> Result<(), String> {
    crate::text_injector::insert_text(text, &|| {
        app.clipboard()
            .write_text(text)
            .map_err(|error| format!("failed to write delivered text to clipboard: {error}"))
    })
}

#[tauri::command]
#[instrument(skip(app), fields(text_len = text.len()), ret, err)]
pub async fn insert_text(app: AppHandle, text: String) -> Result<(), String> {
    do_insert_text(&app, &text)
}

#[tauri::command]
#[instrument(skip(app), fields(text_len = text.len()), ret, err)]
pub async fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(&text).map_err(|e| e.to_string())
}
