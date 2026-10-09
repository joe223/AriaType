use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};

/// Streaming PCM callback type - receives i16 samples at system's buffer rate
/// Parameters: (samples, sample_rate, channels)
pub type PcmCallback = Box<dyn Fn(&[i16], u32, u16) + Send>;

struct RecordingHandle {
    stop_tx: mpsc::Sender<()>,
    thread_handle: thread::JoinHandle<Result<(), String>>,
}

struct ChunkedWavWriter {
    spec: hound::WavSpec,
    base_path: PathBuf,
    chunk_index: usize,
    samples_written: u64,
    samples_per_chunk: u64,
    writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
    on_chunk: Option<Box<dyn Fn(PathBuf) + Send>>,
}

impl ChunkedWavWriter {
    fn new(
        base_path: PathBuf,
        spec: hound::WavSpec,
        on_chunk: Option<Box<dyn Fn(PathBuf) + Send>>,
    ) -> Result<Self, String> {
        let samples_per_chunk = spec.sample_rate as u64 * spec.channels as u64 * 5; // 5 seconds

        let mut writer = Self {
            spec,
            base_path,
            chunk_index: 0,
            samples_written: 0,
            samples_per_chunk,
            writer: None,
            on_chunk,
        };
        writer.open_next_chunk()?;
        Ok(writer)
    }

    fn get_chunk_path(&self) -> PathBuf {
        let mut path = self.base_path.clone();
        if let Some(stem) = path.file_stem() {
            let new_name = format!("{}_{}.wav", stem.to_string_lossy(), self.chunk_index);
            path.set_file_name(new_name);
        }
        path
    }

    fn open_next_chunk(&mut self) -> Result<(), String> {
        let path = self.get_chunk_path();
        self.writer = Some(
            hound::WavWriter::create(path, self.spec)
                .map_err(|e| format!("Failed to create chunk WAV: {}", e))?,
        );
        Ok(())
    }

    fn write_sample(&mut self, sample: i16) -> Result<(), String> {
        if let Some(writer) = &mut self.writer {
            let _ = writer.write_sample(sample);
            self.samples_written += 1;

            if self.samples_written >= self.samples_per_chunk {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<(), String> {
        if let Some(w) = self.writer.take() {
            let _ = w.finalize();
            let path = self.get_chunk_path();
            if let Some(cb) = &self.on_chunk {
                cb(path);
            }
            self.chunk_index += 1;
            self.samples_written = 0;
            self.open_next_chunk()?;
        }
        Ok(())
    }

    fn finalize(mut self) -> Result<Option<PathBuf>, String> {
        if let Some(w) = self.writer.take() {
            let _ = w.finalize();
            let path = self.get_chunk_path();
            if self.samples_written > 0 {
                if let Some(cb) = &self.on_chunk {
                    cb(path.clone());
                }
                Ok(Some(path))
            } else {
                // Remove the empty chunk file
                let _ = std::fs::remove_file(&path);
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}

pub struct AudioRecorder {
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    /// Sample rate of the last recording (set after recording starts)
    last_sample_rate: Arc<Mutex<Option<u32>>>,
    /// Number of channels of the last recording
    last_channels: Arc<Mutex<Option<u16>>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            recording_handle: Arc::new(Mutex::new(None)),
            last_sample_rate: Arc::new(Mutex::new(None)),
            last_channels: Arc::new(Mutex::new(None)),
        }
    }

    /// Start recording with streaming PCM callback.
    /// The callback receives raw i16 samples at system's buffer rate.
    /// Returns the sample rate and channels on success.
    pub fn start_streaming<F>(
        &self,
        device_name: Option<String>,
        on_pcm: F,
    ) -> Result<(u32, u16), String>
    where
        F: Fn(&[i16], u32, u16) + Send + Sync + 'static,
    {
        self.start_streaming_with(move || {
            let host = cpal::default_host();
            let device = match device_name.as_deref() {
                None | Some("default") => host
                    .default_input_device()
                    .ok_or("No input device available")?,
                Some(name) => host
                    .input_devices()
                    .map_err(|e| e.to_string())?
                    .find(|d| d.name().ok().as_deref() == Some(name))
                    .or_else(|| host.default_input_device())
                    .ok_or("No input device available")?,
            };

            let config = device
                .default_input_config()
                .map_err(|e| format!("Failed to get input config: {}", e))?;

            let sample_rate = config.sample_rate().0;
            let channels = config.channels();

            let on_pcm = Arc::new(on_pcm);
            let err_fn = |err| error!(error = %err, "audio_stream_error");

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let cb = on_pcm.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[f32], _: &_| {
                                let samples: Vec<i16> = data
                                    .iter()
                                    .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                                    .collect();
                                cb(&samples, sample_rate, channels);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::I16 => {
                    let cb = on_pcm.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[i16], _: &_| {
                                cb(data, sample_rate, channels);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::U16 => {
                    let cb = on_pcm.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[u16], _: &_| {
                                let samples: Vec<i16> =
                                    data.iter().map(|&s| (s as i32 - 32768) as i16).collect();
                                cb(&samples, sample_rate, channels);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                _ => {
                    return Err(format!(
                        "Unsupported sample format: {:?}",
                        config.sample_format()
                    ))
                }
            };

            stream.play().map_err(|e| e.to_string())?;
            Ok((stream, (sample_rate, channels)))
        })
    }

    fn start_streaming_with<S, F>(&self, start_stream: F) -> Result<(u32, u16), String>
    where
        F: FnOnce() -> Result<(S, (u32, u16)), String> + Send + 'static,
    {
        let mut handle_guard = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        if handle_guard.is_some() {
            return Err("Already recording".to_string());
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, u16), String>>();
        let thread_handle = thread::spawn(move || -> Result<(), String> {
            let (stream, params) = match start_stream() {
                Ok(started) => started,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.clone()));
                    return Err(error);
                }
            };
            if ready_tx.send(Ok(params)).is_err() {
                // The caller timed out before the stream became available.
                drop(stream);
                return Ok(());
            }
            info!(
                sample_rate = params.0,
                channels = params.1,
                "streaming_recording_started"
            );
            let _ = stop_rx.recv();
            drop(stream);
            info!("streaming_recording_stopped");
            Ok(())
        });

        const INIT_TIMEOUT_MS: u64 = 500;
        match ready_rx.recv_timeout(Duration::from_millis(INIT_TIMEOUT_MS)) {
            Ok(Ok((sample_rate, channels))) => {
                if let Ok(mut last_sample_rate) = self.last_sample_rate.lock() {
                    *last_sample_rate = Some(sample_rate);
                }
                if let Ok(mut last_channels) = self.last_channels.lock() {
                    *last_channels = Some(channels);
                }
                *handle_guard = Some(RecordingHandle {
                    stop_tx,
                    thread_handle,
                });
                Ok((sample_rate, channels))
            }
            Ok(Err(error)) => {
                let _ = thread_handle.join();
                Err(error)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                warn!(timeout_ms = INIT_TIMEOUT_MS, "audio_device_init_timeout");
                let _ = stop_tx.send(());
                Err("Audio device initialization timed out".to_string())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                thread_handle
                    .join()
                    .map_err(|_| "Recording thread panicked during startup".to_string())??;
                Err("Recording thread terminated before stream startup".to_string())
            }
        }
    }

    pub fn start<F>(
        &self,
        output_path: PathBuf,
        device_name: Option<String>,
        on_chunk: Option<F>,
    ) -> Result<(), String>
    where
        F: Fn(PathBuf) + Send + 'static,
    {
        let mut handle_guard = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;

        if handle_guard.is_some() {
            return Err("Already recording".to_string());
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let thread_handle = thread::spawn(move || -> Result<(), String> {
            let host = cpal::default_host();
            let device = match device_name.as_deref() {
                None | Some("default") => host
                    .default_input_device()
                    .ok_or("No input device available")?,
                Some(name) => host
                    .input_devices()
                    .map_err(|e| e.to_string())?
                    .find(|d| d.name().ok().as_deref() == Some(name))
                    .or_else(|| host.default_input_device())
                    .ok_or("No input device available")?,
            };

            let config = device
                .default_input_config()
                .map_err(|e| format!("Failed to get input config: {}", e))?;

            let spec = hound::WavSpec {
                channels: config.channels(),
                sample_rate: config.sample_rate().0,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let on_chunk_boxed: Option<Box<dyn Fn(PathBuf) + Send>> =
                on_chunk.map(|f| Box::new(f) as Box<dyn Fn(PathBuf) + Send>);

            let writer = Arc::new(Mutex::new(Some(
                ChunkedWavWriter::new(output_path, spec, on_chunk_boxed)
                    .map_err(|e| format!("Failed to create chunked WAV writer: {}", e))?,
            )));

            let writer_clone = writer.clone();
            let err_fn = |err| error!(error = %err, "audio_stream_error");

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let w = writer_clone.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[f32], _: &_| {
                                if let Ok(mut guard) = w.try_lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &sample in data {
                                            let s = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::I16 => {
                    let w = writer_clone.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[i16], _: &_| {
                                if let Ok(mut guard) = w.try_lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &sample in data {
                                            let _ = writer.write_sample(sample);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::U16 => {
                    let w = writer_clone.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[u16], _: &_| {
                                if let Ok(mut guard) = w.try_lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &sample in data {
                                            let s = (sample as i32 - 32768) as i16;
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                _ => {
                    return Err(format!(
                        "Unsupported sample format: {:?}",
                        config.sample_format()
                    ))
                }
            };

            stream.play().map_err(|e| e.to_string())?;
            info!("audio_recording_started");

            // Wait for stop signal
            let _ = stop_rx.recv();

            // Stop stream
            drop(stream);

            // Finalize WAV
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
                }
            }

            info!("audio_recording_stopped");
            Ok(())
        });

        *handle_guard = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let handle = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?
            .take();

        if let Some(handle) = handle {
            let _ = handle.stop_tx.send(());

            // Wait for thread with timeout
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                if handle.thread_handle.is_finished() {
                    return handle
                        .thread_handle
                        .join()
                        .map_err(|_| "Recording thread panicked".to_string())?;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err("Recording thread timed out".to_string())
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording_handle
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    pub fn get_devices() -> Vec<String> {
        // Return empty list to avoid crashes during enumeration
        // Devices will be enumerated when actually needed for recording
        warn!("audio_device_enumeration_disabled");
        Vec::new()
    }

    /// Get the sample rate and channels of the current/last recording
    pub fn get_last_audio_params(&self) -> Option<(u32, u16)> {
        let sr = self.last_sample_rate.lock().ok().and_then(|g| *g);
        let ch = self.last_channels.lock().ok().and_then(|g| *g);
        sr.zip(ch)
    }
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod streaming_start_tests {
    use super::AudioRecorder;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    #[test]
    fn streaming_start_reports_current_session_format() {
        let recorder = AudioRecorder::new();
        assert_eq!(
            recorder
                .start_streaming_with(|| Ok(((), (48_000, 2))))
                .unwrap(),
            (48_000, 2)
        );
        recorder.stop().unwrap();
        let next = recorder
            .start_streaming_with(|| {
                std::thread::sleep(Duration::from_millis(50));
                Ok(((), (16_000, 1)))
            })
            .unwrap();
        recorder.stop().unwrap();
        assert_eq!(next, (16_000, 1));
        assert_eq!(recorder.get_last_audio_params(), Some((16_000, 1)));
    }

    #[test]
    fn streaming_start_returns_initialization_error_and_allows_restart() {
        let recorder = AudioRecorder::new();
        let result = recorder.start_streaming_with(|| {
            Err::<((), (u32, u16)), _>("Microphone access denied".to_string())
        });
        assert_eq!(result, Err("Microphone access denied".to_string()));
        assert!(!recorder.is_recording());
        assert_eq!(recorder.get_last_audio_params(), None);
        assert_eq!(
            recorder
                .start_streaming_with(|| Ok(((), (16_000, 1))))
                .unwrap(),
            (16_000, 1)
        );
        recorder.stop().unwrap();
    }

    #[test]
    fn failed_later_stream_does_not_accept_previous_format() {
        let recorder = AudioRecorder::new();
        recorder
            .start_streaming_with(|| Ok(((), (48_000, 2))))
            .unwrap();
        recorder.stop().unwrap();
        let result = recorder
            .start_streaming_with(|| Err::<((), (u32, u16)), _>("Device disconnected".to_string()));
        assert_eq!(result, Err("Device disconnected".to_string()));
        assert!(!recorder.is_recording());
    }

    #[test]
    fn streaming_start_waits_until_stream_is_playing() {
        let recorder = Arc::new(AudioRecorder::new());
        let ready = Arc::new(AtomicBool::new(false));
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let recorder_clone = recorder.clone();
        let ready_clone = ready.clone();
        let start_thread = std::thread::spawn(move || {
            let result = recorder_clone.start_streaming_with(move || {
                release_rx.recv().unwrap();
                ready_clone.store(true, Ordering::SeqCst);
                Ok(((), (16_000, 1)))
            });
            started_tx.send(result).unwrap();
        });
        assert!(started_rx.recv_timeout(Duration::from_millis(50)).is_err());
        release_tx.send(()).unwrap();
        assert_eq!(
            started_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            (16_000, 1)
        );
        assert!(ready.load(Ordering::SeqCst));
        start_thread.join().unwrap();
        recorder.stop().unwrap();
    }
}
