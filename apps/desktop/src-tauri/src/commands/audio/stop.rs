use tauri::{AppHandle, Manager, State};
use tracing::{debug, info, instrument, warn};

use crate::events::{emit_recording_state, RecordingStatus};
use crate::services::recording_lifecycle::{cleanup_failed_recording_stop, prepare_recording_stop};
use crate::state::app_state::AppState;

use super::shared::{
    await_streaming_task_in_background, emit_recording_error_then_idle,
    flush_pending_chunk_for_stop, send_flushed_chunk_for_stop,
};

#[tauri::command]
#[instrument(skip(app, _state), ret, err)]
pub async fn stop_recording(
    app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let output_path = stop_recording_sync(app.clone())?;
    Ok(output_path)
}

pub fn stop_recording_sync(app: AppHandle) -> Result<Option<String>, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "AppState not available".to_string())?;

    let Some(prepared) = prepare_recording_stop(&state) else {
        return Ok(None);
    };

    let stop_result = state.recorder.lock().stop();
    let streaming_state = state.take_streaming_state(prepared.task_id);
    if let Err(error) = stop_result {
        cleanup_failed_recording_stop(&state, prepared.task_id);
        let streaming_task = streaming_state
            .as_ref()
            .and_then(|stt| stt.streaming_task.lock().take());
        drop(streaming_state);
        if let Some(handle) = &streaming_task {
            handle.abort();
        }
        crate::commands::window::update_pill_visibility(&app);
        tracing::error!(task_id = prepared.task_id, error = %error, "recorder_stop_failed");
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(handle) = streaming_task {
                if let Err(error) = handle.await {
                    tracing::debug!(task_id = prepared.task_id, error = %error, "failed_recording_consumer_stopped");
                }
            }
            let state = app_clone.state::<AppState>();
            state.clear_cancellation(prepared.task_id);
            if state.is_current_recording_task(prepared.task_id) {
                emit_recording_error_then_idle(&app_clone, prepared.task_id).await;
            }
        });
        return Err(error);
    }

    {
        let settings = state.settings.lock();
        let beep_enabled = settings.beep_on_record;
        drop(settings);

        debug!(beep_enabled, "beep_check-stop_recording");
        if beep_enabled {
            debug!("beep_play-stop");
            crate::audio::beep::play_stop_beep();
        }
    }

    if let Some(stt) = streaming_state {
        if let Some(flushed_chunk) = flush_pending_chunk_for_stop(
            &stt.chunk_buffer,
            &stt.processor,
            stt.sample_rate,
            stt.channels,
        ) {
            if let Err(e) = tauri::async_runtime::block_on(send_flushed_chunk_for_stop(
                &stt.audio_tx,
                flushed_chunk,
            )) {
                warn!(task_id = prepared.task_id, error = %e, "audio_tail_flush_enqueue_failed");
            } else {
                info!(
                    task_id = prepared.task_id,
                    "audio_tail_flushed_before_finish"
                );
            }
        }

        info!(task_id = prepared.task_id, "stt_stopping-awaiting_final");
        if let Some(handle) = stt.streaming_task.lock().take() {
            await_streaming_task_in_background(prepared.task_id, handle);
        }
    } else {
        warn!(
            task_id = prepared.task_id,
            "streaming_state_missing-recording_interrupted"
        );
        if state.is_current_recording_task(prepared.task_id) {
            emit_recording_state(&app, RecordingStatus::Idle, prepared.task_id);
        }
    }

    Ok(None)
}
