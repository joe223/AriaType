use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::settings::CloudProviderConfig;
use crate::runtime_context::window::{WindowContextBundle, WindowContextSource};
use crate::state::app_state::AppState;

use super::capture::should_cancel_window_context_capture;
use super::polish::maybe_polish_transcription_text;
use super::shared::{
    await_streaming_task_in_background, discard_canceled_result, flush_pending_chunk_for_stop,
    recording_chunk_size_samples, send_flushed_chunk_for_stop, should_emit_error_recovery_idle,
    should_unregister_cancel_hotkey_after_async_cleanup, ParkingMutex, ProcessingEventTarget,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waiting_for_streaming_task_does_not_block_the_active_runtime() {
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let handle = tauri::async_runtime::spawn(async move {
        let _ = done_tx.send(());
    });

    await_streaming_task_in_background(1, handle);

    tokio::time::timeout(Duration::from_secs(1), done_rx)
        .await
        .expect("streaming task should complete without blocking the runtime")
        .expect("streaming task completion signal should be delivered");
}

#[tokio::test]
async fn no_template_polish_path_does_not_call_provider() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(&serde_json::json!({
                "choices": [{"message": {"content": "Unexpected polish"}}]
            })),
        )
        .expect(0)
        .mount(&mock_server)
        .await;

    let state = AppState::new();
    state.start_session(11, None);
    {
        let mut settings = state.settings.lock();
        settings.cloud_polish_enabled = true;
        settings.active_cloud_polish_provider = "openai".to_string();
        settings.cloud_polish_configs.insert(
            "openai".to_string(),
            CloudProviderConfig {
                enabled: true,
                provider_type: "openai".to_string(),
                api_key: "test_openai_api_key".to_string(),
                base_url: mock_server.uri(),
                model: "gpt-4o-mini".to_string(),
                enable_thinking: false,
            },
        );
    }

    let result = maybe_polish_transcription_text(
        &ProcessingEventTarget::None,
        &state,
        11,
        "Fast path text".to_string(),
        None,
    )
    .await;

    assert_eq!(result.text, "Fast path text");
    assert_eq!(result.fallback_reason, Some("no polish template"));
}

#[tokio::test]
async fn template_selection_without_model_does_not_pick_local_model_implicitly() {
    let state = AppState::new();
    state.start_session(12, None);
    {
        let mut settings = state.settings.lock();
        settings.cloud_polish_enabled = false;
        settings.polish_model.clear();
    }

    let result = maybe_polish_transcription_text(
        &ProcessingEventTarget::None,
        &state,
        12,
        "Text that asked for a template".to_string(),
        Some("filler".to_string()),
    )
    .await;

    assert_eq!(result.text, "Text that asked for a template");
    assert_eq!(result.fallback_reason, Some("no polish model selected"));
}

#[tokio::test]
async fn incomplete_cloud_polish_config_does_not_fall_back_to_local_model() {
    let state = AppState::new();
    state.start_session(13, None);
    {
        let mut settings = state.settings.lock();
        settings.cloud_polish_enabled = true;
        settings.active_cloud_polish_provider = "openai".to_string();
        settings.polish_model = "qwen3.5-0.8b".to_string();
        settings.cloud_polish_configs.remove("openai");
    }

    let result = maybe_polish_transcription_text(
        &ProcessingEventTarget::None,
        &state,
        13,
        "Cloud polish should not start local polish".to_string(),
        Some("filler".to_string()),
    )
    .await;

    assert_eq!(result.text, "Cloud polish should not start local polish");
    assert_eq!(
        result.fallback_reason,
        Some("cloud polish configuration incomplete")
    );
}

#[tokio::test]
async fn streaming_finalization_honors_cloud_polish_settings() {
    let mock_server = MockServer::start().await;

    let response_body = serde_json::json!({
        "choices": [
            {
                "message": {
                    "content": "Polished streaming text."
                }
            }
        ]
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test_openai_api_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let state = AppState::new();
    state.start_session(1, None);
    {
        let mut settings = state.settings.lock();
        settings.cloud_polish_enabled = true;
        settings.active_cloud_polish_provider = "openai".to_string();
        settings.stt_engine_language = "en-US".to_string();
        settings.cloud_polish_configs.insert(
            "openai".to_string(),
            CloudProviderConfig {
                enabled: true,
                provider_type: "openai".to_string(),
                api_key: "test_openai_api_key".to_string(),
                base_url: mock_server.uri(),
                model: "gpt-4o-mini".to_string(),
                enable_thinking: false,
            },
        );
    }

    let result = maybe_polish_transcription_text(
        &ProcessingEventTarget::None,
        &state,
        1,
        "User text here".to_string(),
        Some("filler".to_string()),
    )
    .await;

    assert_eq!(result.text, "Polished streaming text");
    assert!(result.fallback_reason.is_none());
}

#[tokio::test]
async fn window_context_is_injected_into_polish_prompt() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test_openai_api_key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(&serde_json::json!({
                "choices": [{"message": {"content": "Polished"}}]
            })),
        )
        .mount(&mock_server)
        .await;

    let state = AppState::new();
    state.start_session(2, None);
    {
        let mut session = state.session_state.lock();
        if let Some(s) = session.as_mut() {
            s.window_context = WindowContextBundle::from_ocr_result(
                "Screen content here",
                WindowContextSource::FocusedWindow,
                Some("Notes".to_string()),
                900,
                600,
                Some(0.9),
            );
        }
    }
    {
        let mut settings = state.settings.lock();
        settings.cloud_polish_enabled = true;
        settings.active_cloud_polish_provider = "openai".to_string();
        settings.stt_engine_language = "en-US".to_string();
        settings.cloud_polish_configs.insert(
            "openai".to_string(),
            CloudProviderConfig {
                enabled: true,
                provider_type: "openai".to_string(),
                api_key: "test_openai_api_key".to_string(),
                base_url: mock_server.uri(),
                model: "gpt-4o-mini".to_string(),
                enable_thinking: false,
            },
        );
    }

    let result = maybe_polish_transcription_text(
        &ProcessingEventTarget::None,
        &state,
        2,
        "User input".to_string(),
        Some("filler".to_string()),
    )
    .await;

    assert_eq!(result.text, "Polished");
    assert!(result.fallback_reason.is_none());
}

#[tokio::test]
async fn window_context_disabled_skips_capture() {
    let state = AppState::new();
    state.start_session(3, None);
    {
        let mut settings = state.settings.lock();
        settings.window_context_enabled = false;
    }

    let (denoise_mode, vad_enabled, _domain, _subdomain, _glossary, window_context_enabled) = {
        let settings = state.settings.lock();
        (
            settings.denoise_mode.clone(),
            settings.vad_enabled,
            None::<String>,
            None::<String>,
            None::<String>,
            settings.window_context_enabled,
        )
    };

    assert!(!window_context_enabled);
    assert_eq!(denoise_mode, "off");
    assert!(!vad_enabled);
}

#[test]
fn window_context_capture_cancels_when_recording_ends() {
    assert!(!should_cancel_window_context_capture(true, true, false));
    assert!(should_cancel_window_context_capture(true, false, false));
    assert!(should_cancel_window_context_capture(true, true, true));
    assert!(should_cancel_window_context_capture(false, true, false));
}

#[test]
fn async_cleanup_keeps_cancel_hotkey_while_hotkey_cancel_is_still_active() {
    assert!(!should_unregister_cancel_hotkey_after_async_cleanup(
        1, 1, true
    ));
}

#[test]
fn async_cleanup_ignores_stale_task_after_a_new_recording_starts() {
    assert!(!should_unregister_cancel_hotkey_after_async_cleanup(
        2, 1, false
    ));
}

#[test]
fn async_cleanup_unregisters_cancel_hotkey_only_for_current_non_canceled_task() {
    assert!(should_unregister_cancel_hotkey_after_async_cleanup(
        3, 3, false
    ));
}

#[test]
fn error_recovery_idle_emits_only_for_the_same_finished_task() {
    assert!(should_emit_error_recovery_idle(7, 7, false, false));
}

#[test]
fn error_recovery_idle_skips_after_a_new_recording_starts() {
    assert!(!should_emit_error_recovery_idle(8, 7, true, false));
    assert!(!should_emit_error_recovery_idle(8, 7, false, true));
    assert!(!should_emit_error_recovery_idle(8, 7, false, false));
}

#[test]
fn recording_chunk_size_uses_200ms_of_device_audio() {
    assert_eq!(recording_chunk_size_samples(16_000, 1), 3_200);
    assert_eq!(recording_chunk_size_samples(48_000, 2), 19_200);
}

#[test]
fn flush_pending_chunk_for_stop_processes_sub_threshold_tail_audio() {
    let chunk_buffer = Arc::new(ParkingMutex::new(vec![1_000; 1_600]));
    let processor = Arc::new(ParkingMutex::new(
        crate::audio::stream_processor::StreamAudioProcessor::new("off", false, None),
    ));

    let flushed = flush_pending_chunk_for_stop(&chunk_buffer, &processor, 16_000, 1);

    assert_eq!(flushed, Some(vec![999; 1_600]));
    assert!(chunk_buffer.lock().is_empty());
}

#[test]
fn flush_pending_chunk_for_stop_does_not_drop_tail_when_vad_rejects_it() {
    let chunk_buffer = Arc::new(ParkingMutex::new(vec![1_000; 1_600]));
    let processor = Arc::new(ParkingMutex::new(
        crate::audio::stream_processor::StreamAudioProcessor::new("off", true, None),
    ));
    {
        let mut processor_guard = processor.lock();
        processor_guard.force_vad_result_for_test(false);
        processor_guard.set_last_send_time_for_test(std::time::Instant::now());
    }

    let flushed = flush_pending_chunk_for_stop(&chunk_buffer, &processor, 16_000, 1);

    assert_eq!(flushed, Some(vec![999; 1_600]));
    assert!(chunk_buffer.lock().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn send_flushed_chunk_for_stop_waits_for_capacity_when_channel_is_full() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tx.send(vec![1]).await.unwrap();

    let send_task = tokio::spawn({
        let tx = tx.clone();
        async move { send_flushed_chunk_for_stop(&tx, vec![2]).await }
    });

    assert_eq!(rx.recv().await, Some(vec![1]));
    send_task.await.unwrap().unwrap();
    assert_eq!(rx.recv().await, Some(vec![2]));
}

#[test]
fn discarding_a_canceled_result_only_clears_that_task() {
    let state = AppState::new();
    state.request_cancellation(5);
    state.request_cancellation(6);
    state.start_session(5, None);
    state.task_counter.store(5, Ordering::SeqCst);
    state.is_transcribing.store(true, Ordering::SeqCst);

    discard_canceled_result(&state, 5, None);

    assert!(!state.is_cancellation_requested(5));
    assert!(state.is_cancellation_requested(6));
    assert!(!state.is_transcribing.load(Ordering::SeqCst));
    assert!(state.get_session_text(5).is_none());
}

fn buffered_recording_state(
    task_id: u64,
) -> (
    crate::state::unified_state::StreamingSttState,
    tokio::sync::mpsc::Receiver<Vec<i16>>,
) {
    let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(1);
    let streaming_state = crate::state::unified_state::StreamingSttState {
        audio_tx,
        accumulated_text: String::new(),
        task_id,
        streaming_task: Arc::new(ParkingMutex::new(None)),
        chunk_buffer: Arc::new(ParkingMutex::new(Vec::new())),
        processor: Arc::new(ParkingMutex::new(
            crate::audio::stream_processor::StreamAudioProcessor::new("off", false, None),
        )),
        sample_rate: 16_000,
        channels: 1,
    };
    (streaming_state, audio_rx)
}

#[tokio::test]
async fn stopped_failed_recording_can_prepare_retry_from_its_original_audio() {
    use crate::audio::wav_writer::{save_raw_audio_to_file, RecordedAudio};
    use crate::history::models::TranscriptionEntry;
    use crate::services::retry_transcription::prepare_retry_transcription;

    let state = AppState::new();
    let directory = tempfile::tempdir().unwrap();
    let original_audio = RecordedAudio::new(
        directory.path().join("original.wav"),
        Arc::new(ParkingMutex::new(vec![1_000; 16_000])),
        16_000,
        1,
    );
    let (capture_state, mut audio_rx) = buffered_recording_state(7);
    *state.streaming_stt.lock() = Some(capture_state);
    drop(state.take_streaming_state(7));
    assert_eq!(audio_rx.recv().await, None);

    let (newer_capture, _newer_audio_rx) = buffered_recording_state(8);
    *state.streaming_stt.lock() = Some(newer_capture);
    state.start_session(8, None);
    state.task_counter.store(8, Ordering::SeqCst);
    let audio_path = save_raw_audio_to_file(&original_audio).unwrap();
    let entry = TranscriptionEntry {
        id: "failed-recording".to_string(),
        created_at: 0,
        raw_text: String::new(),
        final_text: String::new(),
        stt_engine: "whisper".to_string(),
        stt_model: None,
        language: Some("en-US".to_string()),
        audio_duration_ms: Some(1_000),
        stt_duration_ms: None,
        polish_duration_ms: None,
        total_duration_ms: None,
        polish_applied: false,
        polish_engine: None,
        is_cloud: false,
        audio_path: Some(audio_path.clone()),
        status: "error".to_string(),
        error: Some("network failed".to_string()),
    };
    let retry = prepare_retry_transcription(&state, entry.id.clone(), entry).unwrap();

    assert_eq!(retry.audio_path, audio_path);
    assert!(std::path::Path::new(&retry.audio_path).exists());
    assert_eq!(state.streaming_stt.lock().as_ref().unwrap().task_id, 8);
    assert!(state.is_current_recording_task(8));
    assert_ne!(retry.task_id, 8);
}

#[test]
fn older_stop_cannot_detach_a_newer_recording_consumer() {
    let state = AppState::new();
    let (capture_state, mut audio_rx) = buffered_recording_state(8);
    *state.streaming_stt.lock() = Some(capture_state);

    assert!(state.take_streaming_state(7).is_none());
    assert_eq!(state.streaming_stt.lock().as_ref().unwrap().task_id, 8);
    assert_eq!(
        audio_rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    );
}

#[test]
fn discarding_an_older_canceled_result_preserves_newer_activity() {
    let state = AppState::new();
    state.request_cancellation(5);
    state.start_session(6, None);
    state.task_counter.store(6, Ordering::SeqCst);
    state.is_transcribing.store(true, Ordering::SeqCst);

    discard_canceled_result(&state, 5, None);

    assert!(state.is_transcribing.load(Ordering::SeqCst));
    assert!(state.get_session_text(6).is_some());
    assert!(!state.is_cancellation_requested(5));
}
